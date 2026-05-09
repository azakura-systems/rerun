use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow;
use egui;
use glam;
use macaw::BoundingBox;
use re_log_types::Instance;
use re_sdk_types::Archetype as _;
use re_sdk_types::archetypes::Quad3D;
use re_sdk_types::components::{Quad3DModel, Quad3DThetas};
use re_viewer_context::{
    IdentifiedViewSystem, ViewClass as _, ViewContext, ViewContextCollection, ViewQuery,
    ViewSystemExecutionError, VisualizerExecutionOutput, VisualizerQueryInfo,
    VisualizerReportSeverity, VisualizerSystem,
};

use super::{SpatialViewVisualizerData, UiLabel, UiLabelStyle, UiLabelTarget};

const QUAD_MODEL_DIR: &str = "rerun/quad3d";
const DEFAULT_QUAD_MODEL: &str = "default.glb";
const REQUIRED_QUAD_MODEL_NODES: [&str; 5] = ["body", "fl_act", "fr_act", "rl_act", "rr_act"];

#[derive(Clone)]
struct LoadedQuadModel {
    body: Arc<[re_renderer::renderer::GpuMeshInstance]>,
    fl_act: Arc<[re_renderer::renderer::GpuMeshInstance]>,
    fr_act: Arc<[re_renderer::renderer::GpuMeshInstance]>,
    rl_act: Arc<[re_renderer::renderer::GpuMeshInstance]>,
    rr_act: Arc<[re_renderer::renderer::GpuMeshInstance]>,
}

#[derive(Default)]
pub struct Quad3DVisualizer;

impl IdentifiedViewSystem for Quad3DVisualizer {
    fn identifier() -> re_viewer_context::ViewSystemIdentifier {
        "Quad3D".into()
    }
}

impl VisualizerSystem for Quad3DVisualizer {
    fn visualizer_query_info(
        &self,
        _app_options: &re_viewer_context::AppOptions,
    ) -> VisualizerQueryInfo {
        let queried_components = [
            Quad3D::descriptor_thetas(),
            Quad3D::descriptor_model(),
            Quad3D::descriptor_label(),
            Quad3D::descriptor_show_label(),
        ];

        VisualizerQueryInfo {
            relevant_archetype: Some(Quad3D::name()),
            constraints: re_viewer_context::VisualizabilityConstraints::AnyBuiltinComponent(
                std::iter::once(Quad3D::descriptor_thetas().component).collect(),
            ),
            queried: queried_components.into_iter().collect(),
        }
    }

    fn affinity(&self) -> Option<re_sdk_types::ViewClassIdentifier> {
        Some(crate::SpatialView3D::identifier())
    }

    fn execute(
        &self,
        ctx: &ViewContext<'_>,
        query: &ViewQuery<'_>,
        context_systems: &ViewContextCollection,
    ) -> Result<VisualizerExecutionOutput, ViewSystemExecutionError> {
        let mut output = VisualizerExecutionOutput::default();
        let mut spatial_data = SpatialViewVisualizerData::default();
        let mut mesh_instances = Vec::new();
        let mut reported_model_errors = HashSet::new();

        use super::entity_iterator::process_archetype;
        process_archetype::<Quad3D, _, _>(
            ctx,
            query,
            context_systems,
            &output,
            self,
            |query_ctx, spatial_ctx, results| {
                let entity_path = query_ctx.target_entity_path;
                let quad_transform = spatial_ctx
                    .transform_info
                    .single_transform_required_for_entity(entity_path, Quad3D::name())
                    .as_affine3a();

                let all_thetas = results.iter_required(Quad3D::descriptor_thetas().component);
                let all_models = results.iter_optional(Quad3D::descriptor_model().component);
                let all_labels = results.iter_optional(Quad3D::descriptor_label().component);
                let all_show_labels =
                    results.iter_optional(Quad3D::descriptor_show_label().component);

                let mut logged_model = None;
                let mut logged_model_index = None;
                for (index, values) in all_models.slice::<String>() {
                    for value in values.iter() {
                        if logged_model_index.is_none_or(|current| current <= index) {
                            logged_model = Some(value.to_string());
                            logged_model_index = Some(index);
                        }
                    }
                }

                let mut label = None;
                let mut label_index = None;
                for (index, values) in all_labels.slice::<String>() {
                    for value in values.iter() {
                        if label_index.is_none_or(|current| current <= index) {
                            label = Some(value.to_string());
                            label_index = Some(index);
                        }
                    }
                }

                let mut show_label = true;
                let mut show_label_index = None;
                for (index, values) in all_show_labels.slice::<bool>() {
                    for value in values.iter() {
                        if show_label_index.is_none_or(|current| current <= index) {
                            show_label = value;
                            show_label_index = Some(index);
                        }
                    }
                }

                let mut model = None;
                for (((_, fls), (_, frs)), ((_, rls), (_, rrs))) in all_thetas
                    .slice_from_struct_field::<f32>("fl")
                    .zip(all_thetas.slice_from_struct_field::<f32>("fr"))
                    .zip(
                        all_thetas
                            .slice_from_struct_field::<f32>("rl")
                            .zip(all_thetas.slice_from_struct_field::<f32>("rr")),
                    )
                {
                    if !fls.is_empty() && model.is_none() {
                        match load_quad_model_for_logged_model(
                            logged_model.as_deref(),
                            query_ctx.render_ctx(),
                        ) {
                            Ok(loaded_model) => {
                                model = Some(loaded_model);
                            }
                            Err(err) => {
                                if reported_model_errors.insert(err.clone()) {
                                    results.report_for_component(
                                        Quad3D::descriptor_model().component,
                                        VisualizerReportSeverity::Error,
                                        err,
                                    );
                                }
                                break;
                            }
                        }
                    }

                    let Some(model) = &model else {
                        continue;
                    };

                    for (((fl, fr), rl), rr) in fls.iter().zip(frs).zip(rls).zip(rrs) {
                        let picking_instance_hash =
                            re_entity_db::InstancePathHash::entity_all(entity_path);
                        let outline_mask_ids =
                            spatial_ctx.highlight.index_outline_mask(Instance::ALL);
                        let picking_layer_id = re_view::picking_layer_id_from_instance_path_hash(
                            picking_instance_hash,
                        );
                        push_quad_model_instances(
                            &mut mesh_instances,
                            model,
                            quad_transform,
                            [*fl, *fr, *rl, *rr],
                            outline_mask_ids,
                            picking_layer_id,
                        );
                        let tracking_bbox = model.tracking_bounding_box();
                        spatial_data.add_bounding_box(
                            entity_path.hash(),
                            tracking_bbox,
                            quad_transform,
                        );
                        if show_label {
                            if let Some(label) = &label
                                && !label.is_empty()
                            {
                                spatial_data.ui_labels.push(UiLabel {
                                    text: label.clone(),
                                    style: UiLabelStyle::Default,
                                    target: UiLabelTarget::Position3D(
                                        quad_transform.transform_point3(tracking_bbox.center()),
                                    ),
                                    labeled_instance: picking_instance_hash,
                                    visualizer_instruction: spatial_ctx.visualizer_instruction,
                                });
                            }
                        }
                    }
                }

                Ok(())
            },
        )?;

        if !mesh_instances.is_empty() {
            output.draw_data.push(
                re_renderer::renderer::MeshDrawData::new(
                    ctx.viewer_ctx.render_ctx(),
                    &mesh_instances,
                )?
                .into(),
            );
        }

        Ok(output.with_visualizer_data(spatial_data))
    }
}

pub fn register_quad3d_component_uis(registry: &mut re_viewer_context::ComponentUiRegistry) {
    registry.add_singleline_edit_or_view_for_component::<Quad3DModel>(
        Quad3D::descriptor_model().component,
        |_ctx, ui, _component_descriptor, value| quad_model_selector_ui(ui, value),
    );
    registry.add_singleline_edit_or_view_for_component::<Quad3DThetas>(
        Quad3D::descriptor_thetas().component,
        |_ctx, ui, _component_descriptor, value| quad_thetas_ui(ui, value),
    );
}

pub fn default_quad3d_model_component() -> Quad3DModel {
    Quad3DModel::from(default_quad_model_for_blueprint())
}

fn quad_model_selector_ui(
    ui: &mut egui::Ui,
    value: &mut re_viewer_context::MaybeMutRef<'_, Quad3DModel>,
) -> egui::Response {
    let Some(value) = value.as_mut() else {
        return ui.label(value.as_str());
    };

    ui.horizontal(|ui| {
        let available_models = available_quad_models();
        let current_model = normalize_logged_model(value.as_str());
        let mut selected = current_model
            .or_else(|| default_quad_model_from(&available_models))
            .unwrap_or_default();
        let previous_selected = selected.clone();

        let mut response = egui::ComboBox::from_id_salt("quad3d_model_selector")
            .selected_text(if selected.is_empty() {
                "No .glb files found"
            } else {
                selected.as_str()
            })
            .show_ui(ui, |ui| {
                if available_models.is_empty() {
                    ui.label("No .glb files found");
                }
                for model in available_models {
                    ui.selectable_value(&mut selected, model.clone(), model);
                }
            })
            .response;

        if selected != previous_selected {
            *value = Quad3DModel::from(selected);
            response.mark_changed();
        }

        response
    })
    .inner
}

fn quad_thetas_ui(
    ui: &mut egui::Ui,
    value: &mut re_viewer_context::MaybeMutRef<'_, Quad3DThetas>,
) -> egui::Response {
    if let Some(thetas) = value.as_mut() {
        let mut edited = false;
        let mut response = ui
            .horizontal(|ui| {
                edited |= theta_drag_value(ui, "fl", &mut thetas.fl.0).changed();
                edited |= theta_drag_value(ui, "fr", &mut thetas.fr.0).changed();
                edited |= theta_drag_value(ui, "rl", &mut thetas.rl.0).changed();
                edited |= theta_drag_value(ui, "rr", &mut thetas.rr.0).changed();
            })
            .response;

        if edited {
            response.mark_changed();
        }

        response
    } else {
        ui.horizontal(|ui| {
            ui.label(format!(
                "fl {:.3}  fr {:.3}  rl {:.3}  rr {:.3}",
                value.fl.0, value.fr.0, value.rl.0, value.rr.0
            ));
        })
        .response
    }
}

fn theta_drag_value(ui: &mut egui::Ui, label: &str, value: &mut f32) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.01).max_decimals(3))
    })
    .inner
}

fn default_quad_model_from(available_models: &[String]) -> Option<String> {
    if available_models
        .iter()
        .any(|model| model == DEFAULT_QUAD_MODEL)
    {
        return Some(DEFAULT_QUAD_MODEL.to_owned());
    }

    available_models.first().cloned()
}

fn default_quad_model_for_blueprint() -> String {
    default_quad_model_from(&available_quad_models())
        .unwrap_or_else(|| DEFAULT_QUAD_MODEL.to_owned())
}

fn default_quad_model() -> Result<String, String> {
    default_quad_model_from(&available_quad_models()).ok_or_else(|| {
        format!(
            "No Quad3D .glb models found in {}",
            quad_model_dir_display()
        )
    })
}

fn load_quad_model_for_logged_model(
    model: Option<&str>,
    render_ctx: &re_renderer::RenderContext,
) -> Result<LoadedQuadModel, String> {
    let model = match model.and_then(normalize_logged_model) {
        Some(model) => model,
        None => default_quad_model()?,
    };
    load_quad_model(model, render_ctx)
}

fn normalize_logged_model(model: &str) -> Option<String> {
    let model = model.trim();
    if model.is_empty() || model == "default" {
        return None;
    }

    let file_name = Path::new(model)
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or(model);
    if file_name.ends_with(".glb") {
        Some(file_name.to_owned())
    } else {
        Some(format!("{file_name}.glb"))
    }
}

fn load_quad_model(
    model_file_name: String,
    render_ctx: &re_renderer::RenderContext,
) -> Result<LoadedQuadModel, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, LoadedQuadModel>>> = OnceLock::new();

    // TODO: Consider invalidating cached models when the underlying GLB file changes.
    // The current cache is intentionally simple, but model edits require a viewer restart.
    let cache = CACHE.get_or_init(Default::default);
    {
        let cache = lock_quad_model_cache(cache)?;
        if let Some(model) = cache.get(&model_file_name) {
            return Ok(model.clone());
        }
    }

    let path = match quad_model_path(&model_file_name) {
        Ok(path) => path,
        Err(err) => {
            let err = format!(
                "Failed to resolve Quad3D model directory: {}",
                re_error::format(&err)
            );
            re_log::error_once!("{err}");
            return Err(err);
        }
    };
    let model = match load_quad_model_from_path(&path, render_ctx) {
        Ok(model) => {
            re_log::debug!("Loaded Quad3D model from {}", path.display());
            model
        }
        Err(err) => {
            let err = format!(
                "Failed to load Quad3D model from {}: {}",
                path.display(),
                re_error::format(&err)
            );
            re_log::error_once!("{err}");
            return Err(err);
        }
    };

    let mut cache = lock_quad_model_cache(cache)?;
    Ok(cache.entry(model_file_name).or_insert(model).clone())
}

fn lock_quad_model_cache<'a>(
    cache: &'a Mutex<HashMap<String, LoadedQuadModel>>,
) -> Result<std::sync::MutexGuard<'a, HashMap<String, LoadedQuadModel>>, String> {
    cache.lock().map_err(|err| {
        let err = format!("Quad3D model cache is poisoned: {err}");
        re_log::error!("{err}");
        err
    })
}

fn quad_model_dir() -> anyhow::Result<PathBuf> {
    let assets = azk_assets::Assets::new()?;
    let model_dir = assets.join(QUAD_MODEL_DIR);
    std::fs::create_dir_all(&model_dir)?;
    Ok(model_dir)
}

fn quad_model_path(model_file_name: &str) -> anyhow::Result<PathBuf> {
    Ok(quad_model_dir()?.join(model_file_name))
}

fn available_quad_models() -> Vec<String> {
    // TODO: Cache this model list and refresh it explicitly or periodically.
    // This is called from UI/default lookup paths, so scanning the filesystem here
    // every frame is unnecessary even if the model directory is usually tiny.
    let mut models = quad_model_dir()
        .ok()
        .and_then(|model_dir| std::fs::read_dir(model_dir).ok())
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("glb") {
                return None;
            }
            path.file_name()
                .and_then(|file_name| file_name.to_str())
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    models.sort();
    models
}

fn quad_model_dir_display() -> String {
    quad_model_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|err| format!("<unavailable: {}>", re_error::format(&err)))
}

fn load_quad_model_from_path(
    path: &std::path::Path,
    render_ctx: &re_renderer::RenderContext,
) -> anyhow::Result<LoadedQuadModel> {
    let bytes = std::fs::read(path)?;
    let cpu_model = re_renderer::importer::gltf::load_gltf_from_buffer(
        &path.display().to_string(),
        &bytes,
        render_ctx,
    )?;
    let mesh_instances = cpu_model.into_gpu_meshes(render_ctx)?;
    let mesh_node_names = mesh_node_names_from_gltf(&bytes)?;

    LoadedQuadModel::from_named_mesh_instances(mesh_node_names, mesh_instances)
}

impl LoadedQuadModel {
    fn from_named_mesh_instances(
        mesh_node_names: Vec<String>,
        mesh_instances: Vec<re_renderer::renderer::GpuMeshInstance>,
    ) -> anyhow::Result<Self> {
        if mesh_node_names.len() != mesh_instances.len() {
            anyhow::bail!(
                "GLB mesh node count mismatch: {} node names, {} mesh instances",
                mesh_node_names.len(),
                mesh_instances.len()
            );
        }

        let mut body = Vec::new();
        let mut fl_act = Vec::new();
        let mut fr_act = Vec::new();
        let mut rl_act = Vec::new();
        let mut rr_act = Vec::new();

        for (name, mesh_instance) in mesh_node_names.into_iter().zip(mesh_instances) {
            match name.as_str() {
                "body" => body.push(mesh_instance),
                "fl_act" => fl_act.push(mesh_instance),
                "fr_act" => fr_act.push(mesh_instance),
                "rl_act" => rl_act.push(mesh_instance),
                "rr_act" => rr_act.push(mesh_instance),
                _ => {
                    re_log::debug!("Ignoring Quad3D GLB mesh node {name:?}");
                }
            }
        }

        validate_required_quad_model_node("body", &body)?;
        validate_required_quad_model_node("fl_act", &fl_act)?;
        validate_required_quad_model_node("fr_act", &fr_act)?;
        validate_required_quad_model_node("rl_act", &rl_act)?;
        validate_required_quad_model_node("rr_act", &rr_act)?;

        Ok(Self {
            body: body.into(),
            fl_act: fl_act.into(),
            fr_act: fr_act.into(),
            rl_act: rl_act.into(),
            rr_act: rr_act.into(),
        })
    }

    fn tracking_bounding_box(&self) -> BoundingBox {
        let mut bbox = BoundingBox::nothing();

        extend_mesh_instance_bbox(&mut bbox, &self.body, glam::Affine3A::IDENTITY);

        bbox
    }
}

fn validate_required_quad_model_node(
    name: &str,
    parts: &[re_renderer::renderer::GpuMeshInstance],
) -> anyhow::Result<()> {
    if parts.is_empty() {
        anyhow::bail!(
            "Quad3D GLB is missing required mesh node '{name}'. Expected mesh nodes: {}.",
            REQUIRED_QUAD_MODEL_NODES.join(", ")
        );
    }
    if parts.len() > 1 {
        anyhow::bail!(
            "Quad3D GLB has {} mesh nodes named '{name}', but exactly one is required. Expected mesh nodes: {}.",
            parts.len(),
            REQUIRED_QUAD_MODEL_NODES.join(", ")
        );
    }
    Ok(())
}

fn extend_mesh_instance_bbox(
    bbox: &mut BoundingBox,
    mesh_instances: &[re_renderer::renderer::GpuMeshInstance],
    extra_transform: glam::Affine3A,
) {
    for mesh_instance in mesh_instances {
        *bbox = bbox.union(
            mesh_instance
                .gpu_mesh
                .bbox
                .transform_affine3(&(mesh_instance.world_from_mesh * extra_transform)),
        );
    }
}

fn mesh_node_names_from_gltf(bytes: &[u8]) -> anyhow::Result<Vec<String>> {
    let gltf = gltf::Gltf::from_slice(bytes)?;
    let mut names = Vec::new();

    for scene in gltf.scenes() {
        for node in scene.nodes() {
            collect_mesh_node_names(&node, &mut names);
        }
    }

    Ok(names)
}

fn collect_mesh_node_names(node: &gltf::Node<'_>, names: &mut Vec<String>) {
    for child in node.children() {
        collect_mesh_node_names(&child, names);
    }

    if node.mesh().is_some() {
        names.push(node.name().unwrap_or("<unnamed>").to_owned());
    }
}

fn push_quad_model_instances(
    mesh_instances: &mut Vec<re_renderer::renderer::GpuMeshInstance>,
    model: &LoadedQuadModel,
    quad_transform: glam::Affine3A,
    thetas: [f32; 4],
    outline_mask_ids: re_renderer::OutlineMaskPreference,
    picking_layer_id: re_renderer::PickingLayerId,
) {
    push_model_part_instances(
        mesh_instances,
        &model.body,
        quad_transform,
        outline_mask_ids,
        picking_layer_id,
    );

    for (parts, theta) in [
        (&model.fl_act, thetas[0]),
        (&model.fr_act, thetas[1]),
        (&model.rl_act, thetas[2]),
        (&model.rr_act, thetas[3]),
    ] {
        let spin = glam::Affine3A::from_quat(glam::Quat::from_rotation_z(
            theta.rem_euclid(std::f32::consts::TAU),
        ));
        push_rotor_instances(
            mesh_instances,
            parts,
            quad_transform,
            spin,
            outline_mask_ids,
            picking_layer_id,
        );
    }
}

fn push_model_part_instances(
    mesh_instances: &mut Vec<re_renderer::renderer::GpuMeshInstance>,
    source_instances: &[re_renderer::renderer::GpuMeshInstance],
    transform: glam::Affine3A,
    outline_mask_ids: re_renderer::OutlineMaskPreference,
    picking_layer_id: re_renderer::PickingLayerId,
) {
    for mesh_instance in source_instances {
        mesh_instances.push(re_renderer::renderer::GpuMeshInstance {
            gpu_mesh: mesh_instance.gpu_mesh.clone(),
            world_from_mesh: transform * mesh_instance.world_from_mesh,
            additive_tint: re_renderer::Color32::BLACK,
            outline_mask_ids,
            picking_layer_id,
            cull_mode: mesh_instance.cull_mode,
        });
    }
}

fn push_rotor_instances(
    mesh_instances: &mut Vec<re_renderer::renderer::GpuMeshInstance>,
    source_instances: &[re_renderer::renderer::GpuMeshInstance],
    quad_transform: glam::Affine3A,
    spin: glam::Affine3A,
    outline_mask_ids: re_renderer::OutlineMaskPreference,
    picking_layer_id: re_renderer::PickingLayerId,
) {
    for mesh_instance in source_instances {
        mesh_instances.push(re_renderer::renderer::GpuMeshInstance {
            gpu_mesh: mesh_instance.gpu_mesh.clone(),
            world_from_mesh: quad_transform * mesh_instance.world_from_mesh * spin,
            additive_tint: re_renderer::Color32::BLACK,
            outline_mask_ids,
            picking_layer_id,
            cull_mode: mesh_instance.cull_mode,
        });
    }
}
