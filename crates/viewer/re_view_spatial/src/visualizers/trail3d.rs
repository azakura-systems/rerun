use egui;
use glam;
use macaw::BoundingBox;
use re_log_types::{Instance, TimeType};
use re_sdk_types::Archetype as _;
use re_sdk_types::archetypes::Trail3D;
use re_sdk_types::components::{Trail3DColor, Trail3DLength, Trail3DMagnitudeRange};
use re_viewer_context::{
    IdentifiedViewSystem, ViewClass as _, ViewContext, ViewContextCollection, ViewQuery,
    ViewSystemExecutionError, VisualizerExecutionOutput, VisualizerQueryInfo,
    VisualizerReportSeverity, VisualizerSystem,
};

use super::SpatialViewVisualizerData;
use super::utilities::{
    spatial_view_kind_from_affinity, spatial_view_kind_from_view_class,
    transform_info_for_archetype_or_report_error,
};

const DEFAULT_TRAIL_LENGTH_SECONDS: f64 = 5.0;
const DEFAULT_TRAIL_LENGTH_TICKS: u64 = 500;
const MIN_TRAIL_LENGTH_SECONDS: f64 = 0.001;
const MAX_TRAIL_LENGTH_SECONDS: f64 = 60.0 * 60.0;
const MIN_TRAIL_LENGTH_TICKS: u64 = 1;
const MAX_TRAIL_LENGTH_TICKS: u64 = 1_000_000;
const DEFAULT_TRAIL_COLOR_RGB: [u8; 3] = [80, 220, 140];
#[derive(Default)]
pub struct Trail3DVisualizer;

enum TrailRenderData {
    Solid(SolidTrailRenderData),
    Magnitude(MagnitudeTrailRenderData),
}

struct SolidTrailRenderData {
    entity_path: re_log_types::EntityPath,
    points: Vec<glam::Vec3>,
    color: re_renderer::Color32,
    radius: re_renderer::Size,
    world_from_obj: glam::Affine3A,
}

struct MagnitudeTrailRenderData {
    entity_path: re_log_types::EntityPath,
    samples: Vec<MagnitudeTrailSample>,
    color_mapping: TrailColorMapping,
    radius: re_renderer::Size,
    world_from_obj: glam::Affine3A,
}

struct MagnitudeTrailSample {
    point: glam::Vec3,
    magnitude: f64,
}

#[derive(Clone, Copy)]
struct TrailColorMapping {
    range: [f64; 2],
    colormap: re_renderer::Colormap,
}

impl TrailRenderData {
    fn entity_path(&self) -> &re_log_types::EntityPath {
        match self {
            Self::Solid(trail) => &trail.entity_path,
            Self::Magnitude(trail) => &trail.entity_path,
        }
    }

    fn world_from_obj(&self) -> glam::Affine3A {
        match self {
            Self::Solid(trail) => trail.world_from_obj,
            Self::Magnitude(trail) => trail.world_from_obj,
        }
    }

    fn extend_bounding_box(&self, bounding_box: &mut BoundingBox) {
        match self {
            Self::Solid(trail) => {
                for point in &trail.points {
                    bounding_box.extend(*point);
                }
            }
            Self::Magnitude(trail) => {
                for sample in &trail.samples {
                    bounding_box.extend(sample.point);
                }
            }
        }
    }

    fn strip_count(&self) -> usize {
        match self {
            Self::Solid(_) => 1,
            Self::Magnitude(trail) => trail.samples.len().saturating_sub(1),
        }
    }

    fn vertex_count(&self) -> usize {
        match self {
            Self::Solid(trail) => trail.points.len(),
            Self::Magnitude(trail) => trail.samples.len().saturating_sub(1) * 2,
        }
    }
}

impl IdentifiedViewSystem for Trail3DVisualizer {
    fn identifier() -> re_viewer_context::ViewSystemIdentifier {
        "Trail3D".into()
    }
}

impl VisualizerSystem for Trail3DVisualizer {
    fn visualizer_query_info(
        &self,
        _app_options: &re_viewer_context::AppOptions,
    ) -> VisualizerQueryInfo {
        VisualizerQueryInfo {
            relevant_archetype: Some(Trail3D::name()),
            constraints: re_viewer_context::VisualizabilityConstraints::AnyBuiltinComponent(
                std::iter::once(Trail3D::descriptor_point().component).collect(),
            ),
            queried: Trail3D::all_components().iter().cloned().collect(),
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
        let transforms = context_systems.get::<crate::TransformTreeContext>(&output)?;
        let view_kind = spatial_view_kind_from_view_class(ctx.view_class_identifier);
        let archetype_kind = spatial_view_kind_from_affinity(self.affinity());
        let mut spatial_data = SpatialViewVisualizerData::default();
        let mut line_builder = re_renderer::LineDrawableBuilder::new(ctx.viewer_ctx.render_ctx());
        line_builder.radius_boost_in_ui_points_for_outlines(
            re_view::SIZE_BOOST_IN_POINTS_FOR_LINE_OUTLINES,
        );

        let mut has_trails = false;
        let point_component = Trail3D::descriptor_point().component;
        let magnitude_component = Trail3D::descriptor_magnitude().component;
        let magnitude_range_component = Trail3D::descriptor_magnitude_range().component;
        let color_component = Trail3D::descriptor_color().component;
        let radius_component = Trail3D::descriptor_radius().component;
        let length_component = Trail3D::descriptor_length().component;

        // TODO(#azk): Revisit whether Rerun grows a native helper for historical spatial
        // visualizers. `process_archetype` is latest-at shaped, while Trail3D needs range
        // queries for historical points/magnitudes plus latest-at style components.
        for (data_result, instruction) in query.iter_visualizer_instruction_for(Self::identifier())
        {
            let Some(transform_info) = transform_info_for_archetype_or_report_error(
                &data_result.entity_path,
                transforms,
                archetype_kind,
                view_kind,
                &instruction.id,
                &output,
            ) else {
                continue;
            };
            let world_from_obj = transform_info
                .single_transform_required_for_entity(&data_result.entity_path, Trail3D::name())
                .as_affine3a();

            let latest_query = re_chunk_store::LatestAtQuery::new(query.timeline, query.latest_at);
            let latest_results = re_view::latest_at_with_blueprint_resolved_data(
                ctx,
                None,
                &latest_query,
                data_result,
                [
                    magnitude_range_component,
                    color_component,
                    radius_component,
                    length_component,
                ],
                Some(instruction),
            );
            let latest_results =
                re_view::BlueprintResolvedResults::LatestAt(latest_query, latest_results);
            let latest_results = re_view::VisualizerInstructionQueryResults::new(
                instruction,
                &latest_results,
                &output,
            );
            let color = latest_color(&latest_results, color_component);
            let magnitude_range =
                latest_magnitude_range(&latest_results, magnitude_range_component);
            let radius = latest_radius(&latest_results, radius_component)
                .map(|radius| process_radius(&data_result.entity_path, radius))
                .unwrap_or_else(default_trail3d_radius_size);
            let length =
                effective_trail_length(&latest_results, &data_result.entity_path, length_component);
            let history_ticks = trail_history_ticks(
                &latest_results,
                &data_result.entity_path,
                ctx.recording().timeline_type(&query.timeline),
                length,
            );
            // TODO(#azk): Replace this bounded range query with a reverse cursor if Rerun exposes
            // one for visualizers. Ideally Trail3D would walk backward and stop as soon as the
            // length limit is reached instead of materializing the whole configured window first.
            let trail_query_range = re_sdk_types::datatypes::TimeRange {
                start: re_sdk_types::datatypes::TimeRangeBoundary::CursorRelative(
                    re_sdk_types::datatypes::TimeInt::from(-history_ticks),
                ),
                end: re_sdk_types::datatypes::TimeRangeBoundary::AT_CURSOR,
            };
            let range_query = re_chunk::RangeQuery::new(
                query.timeline,
                re_log_types::AbsoluteTimeRange::from_relative_time_range(
                    &trail_query_range,
                    query.latest_at,
                ),
            );

            let results = if color.is_magnitude() {
                re_view::range_with_blueprint_resolved_data(
                    ctx,
                    None,
                    &range_query,
                    data_result,
                    [point_component, magnitude_component],
                    instruction,
                )
            } else {
                re_view::range_with_blueprint_resolved_data(
                    ctx,
                    None,
                    &range_query,
                    data_result,
                    [point_component],
                    instruction,
                )
            };
            let results = re_view::BlueprintResolvedResults::Range(range_query.clone(), results);
            let results =
                re_view::VisualizerInstructionQueryResults::new(instruction, &results, &output);

            if color.is_magnitude() {
                let points = collect_timed_trail_points(&results, point_component);
                if points.len() < 2 {
                    continue;
                }

                let first_point_index = points.first().map(|(index, _)| *index);
                let magnitudes =
                    collect_trail_magnitudes_from(&results, magnitude_component, first_point_index);
                let Some(color_mapping) = color_mapping_for_magnitudes(
                    &latest_results,
                    &data_result.entity_path,
                    &magnitudes,
                    magnitude_range,
                    color.colormap(),
                ) else {
                    continue;
                };

                let samples =
                    align_magnitude_trail_samples(points, &magnitudes, color_mapping.range[0]);
                if samples.len() < 2 {
                    continue;
                }

                let trail = TrailRenderData::Magnitude(MagnitudeTrailRenderData {
                    entity_path: data_result.entity_path.clone(),
                    samples,
                    color_mapping,
                    radius,
                    world_from_obj,
                });
                emit_trail(&mut line_builder, &mut spatial_data, query, trail)?;
                has_trails = true;
            } else {
                let points = collect_trail_points(&results, point_component);
                if points.len() < 2 {
                    continue;
                }

                let trail = TrailRenderData::Solid(SolidTrailRenderData {
                    entity_path: data_result.entity_path.clone(),
                    points,
                    color: sdk_color_to_renderer(color.color),
                    radius,
                    world_from_obj,
                });
                emit_trail(&mut line_builder, &mut spatial_data, query, trail)?;
                has_trails = true;
            }
        }

        if !has_trails {
            return Ok(output);
        }

        output.draw_data.push(line_builder.into_draw_data()?.into());
        Ok(output.with_visualizer_data(spatial_data))
    }
}

fn emit_trail(
    line_builder: &mut re_renderer::LineDrawableBuilder<'_>,
    spatial_data: &mut SpatialViewVisualizerData,
    query: &ViewQuery<'_>,
    trail: TrailRenderData,
) -> Result<(), ViewSystemExecutionError> {
    // TODO(#azk): If many Trail3D entities are visible, consider a counting pass or staging pass
    // to reserve all strips/vertices once globally. Per-trail reserve avoids staging memory but
    // may grow renderer buffers more often.
    line_builder.reserve_strips(trail.strip_count())?;
    line_builder.reserve_vertices(trail.vertex_count())?;

    let entity_path = trail.entity_path();
    let world_from_obj = trail.world_from_obj();
    let outline_mask_ids = query
        .highlights
        .entity_outline_mask(entity_path.hash())
        .index_outline_mask(Instance::ALL);

    let mut bounding_box = BoundingBox::nothing();
    trail.extend_bounding_box(&mut bounding_box);
    spatial_data.add_bounding_box(entity_path.hash(), bounding_box, world_from_obj);

    add_trail_segments(line_builder, trail, outline_mask_ids);

    Ok(())
}

pub fn register_trail3d_component_uis(registry: &mut re_viewer_context::ComponentUiRegistry) {
    registry.add_singleline_edit_or_view_for_component::<Trail3DColor>(
        Trail3D::descriptor_color().component,
        |ctx, ui, _component_descriptor, value| trail_color_ui(ctx, ui, value),
    );
    registry.add_singleline_edit_or_view_for_component::<Trail3DMagnitudeRange>(
        Trail3D::descriptor_magnitude_range().component,
        |_ctx, ui, _component_descriptor, value| trail_magnitude_range_ui(ui, value),
    );
    registry.add_singleline_edit_or_view_for_component::<Trail3DLength>(
        Trail3D::descriptor_length().component,
        |_ctx, ui, _component_descriptor, value| trail_length_ui(ui, value),
    );
}

pub fn default_trail3d_color_component() -> Trail3DColor {
    Trail3DColor::solid(re_sdk_types::components::Color::from_rgb(
        DEFAULT_TRAIL_COLOR_RGB[0],
        DEFAULT_TRAIL_COLOR_RGB[1],
        DEFAULT_TRAIL_COLOR_RGB[2],
    ))
}

pub fn default_trail3d_magnitude_range_component() -> Trail3DMagnitudeRange {
    Trail3DMagnitudeRange::auto()
}

pub fn default_trail3d_radius_component() -> re_sdk_types::components::Radius {
    re_sdk_types::components::Radius::default()
}

pub fn default_trail3d_length_component() -> Trail3DLength {
    Trail3DLength::new(DEFAULT_TRAIL_LENGTH_SECONDS, DEFAULT_TRAIL_LENGTH_TICKS)
}

fn trail_color_ui(
    ctx: &re_viewer_context::StoreViewContext<'_>,
    ui: &mut egui::Ui,
    value: &mut re_viewer_context::MaybeMutRef<'_, Trail3DColor>,
) -> egui::Response {
    let Some(value) = value.as_mut() else {
        return trail_color_view_ui(ctx, ui, value);
    };

    ui.horizontal(|ui| {
        let mut is_magnitude = value.is_magnitude();
        let was_magnitude = is_magnitude;

        let mut response = egui::ComboBox::from_id_salt("trail3d_color_selector")
            .selected_text(if is_magnitude { "Magnitude" } else { "Solid" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut is_magnitude, false, "Solid");
                ui.selectable_value(&mut is_magnitude, true, "Magnitude");
            })
            .response;

        if is_magnitude != was_magnitude {
            *value = if is_magnitude {
                Trail3DColor::magnitude(value.colormap())
            } else {
                Trail3DColor::solid(re_sdk_types::datatypes::Rgba32::from_u32(value.color))
            };
            response.mark_changed();
        }

        if is_magnitude {
            let mut colormap = value.colormap();
            let mut colormap_ref = re_viewer_context::MaybeMutRef::MutRef(&mut colormap);
            let colormap_response =
                re_viewer_context::gpu_bridge::colormap_edit_or_view_ui_with_selection(
                    ctx,
                    ui,
                    &mut colormap_ref,
                    re_sdk_types::ColormapSelection::Standard,
                );
            if colormap_response.changed() {
                *value = Trail3DColor::magnitude(colormap);
            }
            response |= colormap_response;
        } else {
            let mut color = re_sdk_types::datatypes::Rgba32::from_u32(value.color);
            let mut color_ref = re_viewer_context::MaybeMutRef::MutRef(&mut color);
            let color_response = ui.add(re_component_ui::color_swatch::ColorSwatch::new(
                &mut color_ref,
            ));
            if color_response.changed() {
                *value = Trail3DColor::solid(color);
            }
            response |= color_response;
        }

        response
    })
    .inner
}

fn trail_color_view_ui(
    ctx: &re_viewer_context::StoreViewContext<'_>,
    ui: &mut egui::Ui,
    value: &Trail3DColor,
) -> egui::Response {
    if value.is_magnitude() {
        let colormap = value.colormap();
        let mut colormap_ref = re_viewer_context::MaybeMutRef::Ref(&colormap);
        re_viewer_context::gpu_bridge::colormap_edit_or_view_ui_with_selection(
            ctx,
            ui,
            &mut colormap_ref,
            re_sdk_types::ColormapSelection::Standard,
        )
    } else {
        ui.horizontal(|ui| {
            ui.label("Solid");
            let color = re_sdk_types::datatypes::Rgba32::from_u32(value.color);
            let mut color_ref = re_viewer_context::MaybeMutRef::Ref(&color);
            ui.add(re_component_ui::color_swatch::ColorSwatch::new(
                &mut color_ref,
            ))
        })
        .inner
    }
}

fn trail_magnitude_range_ui(
    ui: &mut egui::Ui,
    value: &mut re_viewer_context::MaybeMutRef<'_, Trail3DMagnitudeRange>,
) -> egui::Response {
    let Some(value) = value.as_mut() else {
        return trail_magnitude_range_view_ui(ui, value);
    };

    ui.horizontal(|ui| {
        let mut is_fixed = value.is_fixed();
        let was_fixed = is_fixed;

        let mut response = egui::ComboBox::from_id_salt("trail3d_magnitude_range_selector")
            .selected_text(if is_fixed { "Fixed" } else { "Auto" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut is_fixed, false, "Auto");
                ui.selectable_value(&mut is_fixed, true, "Fixed");
            })
            .response;

        if is_fixed != was_fixed {
            *value = if is_fixed {
                Trail3DMagnitudeRange::fixed(value.fixed_range())
            } else {
                Trail3DMagnitudeRange::auto()
            };
            response.mark_changed();
        }

        if is_fixed {
            let mut min = value.min;
            let mut max = value.max;
            let range = (max - min).abs();
            let speed = (range * 0.01).max(0.001);
            let min_response = ui.add(
                egui::DragValue::new(&mut min)
                    .clamp_existing_to_range(false)
                    .range(f64::NEG_INFINITY..=max)
                    .speed(speed),
            );
            ui.label("-");
            let max_response = ui.add(
                egui::DragValue::new(&mut max)
                    .clamp_existing_to_range(false)
                    .range(min..=f64::INFINITY)
                    .speed(speed),
            );
            if min_response.changed() || max_response.changed() {
                *value = Trail3DMagnitudeRange::fixed([min, max]);
            }
            response |= min_response | max_response;
        }

        response
    })
    .inner
}

fn trail_magnitude_range_view_ui(
    ui: &mut egui::Ui,
    value: &Trail3DMagnitudeRange,
) -> egui::Response {
    if value.is_fixed() {
        ui.label(format!("Fixed [{:.3}, {:.3}]", value.min, value.max))
    } else {
        ui.label("Auto")
    }
}

fn trail_length_ui(
    ui: &mut egui::Ui,
    value: &mut re_viewer_context::MaybeMutRef<'_, Trail3DLength>,
) -> egui::Response {
    let Some(value) = value.as_mut() else {
        return ui.label(format!("{:.3}s / {} ticks", value.seconds, value.ticks));
    };

    ui.horizontal(|ui| {
        let seconds_speed = (value.seconds.abs() * 0.01).max(0.001);
        let mut response = ui
            .add(
                egui::DragValue::new(&mut value.seconds)
                    .clamp_existing_to_range(false)
                    .range(MIN_TRAIL_LENGTH_SECONDS..=MAX_TRAIL_LENGTH_SECONDS)
                    .speed(seconds_speed)
                    .suffix("s"),
            )
            .on_hover_text(format!(
                "Used on temporal timelines. Supported range is {MIN_TRAIL_LENGTH_SECONDS} to {MAX_TRAIL_LENGTH_SECONDS} seconds."
            ));
        response |= ui
            .add(
                egui::DragValue::new(&mut value.ticks)
                    .range(MIN_TRAIL_LENGTH_TICKS..=MAX_TRAIL_LENGTH_TICKS)
                    .speed(1.0)
                    .suffix(" ticks"),
            )
            .on_hover_text(format!(
                "Used on sequence timelines. Supported range is {MIN_TRAIL_LENGTH_TICKS} to {MAX_TRAIL_LENGTH_TICKS} ticks."
            ));
        response
    })
    .inner
}

type TrailTimeIndex = (re_log_types::TimeInt, re_chunk::RowId, usize);
type TimedTrailValue<T> = (TrailTimeIndex, T);

fn collect_trail_points(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    component: re_sdk_types::ComponentIdentifier,
) -> Vec<glam::Vec3> {
    let mut points = Vec::new();

    // TODO(#azk): Confirm the range iterator ordering contract. Solid trails do not need
    // timestamps for rendering, but this assumes row iteration is already chronological.
    // If that is not guaranteed, use the timed collector and sort like magnitude trails do.
    let point_results = results.iter_required(component);
    for (xs, ys, zs) in point_results
        .slice_from_struct_field::<f32>("x")
        .map(|(_, values)| values)
        .zip(
            point_results
                .slice_from_struct_field::<f32>("y")
                .map(|(_, values)| values),
        )
        .zip(
            point_results
                .slice_from_struct_field::<f32>("z")
                .map(|(_, values)| values),
        )
        .map(|((x, y), z)| (x, y, z))
    {
        points.extend(
            xs.iter()
                .zip(ys)
                .zip(zs)
                .map(|((x, y), z)| glam::vec3(*x, *y, *z)),
        );
    }

    points
}

fn collect_timed_trail_points(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    component: re_sdk_types::ComponentIdentifier,
) -> Vec<TimedTrailValue<glam::Vec3>> {
    let mut points = Vec::new();
    let mut previous_index = None;
    let mut needs_sort = false;

    let point_results = results.iter_required(component);
    for (((time, row_id), xs), (_, ys), (_, zs)) in point_results
        .slice_from_struct_field::<f32>("x")
        .zip(point_results.slice_from_struct_field::<f32>("y"))
        .zip(point_results.slice_from_struct_field::<f32>("z"))
        .map(|((x, y), z)| (x, y, z))
    {
        for (sample_index, ((x, y), z)) in xs.iter().zip(ys).zip(zs).enumerate() {
            let index = (time, row_id, sample_index);
            if previous_index.is_some_and(|previous| previous > index) {
                needs_sort = true;
            }
            previous_index = Some(index);
            points.push((index, glam::vec3(*x, *y, *z)));
        }
    }

    if needs_sort {
        points.sort_by_key(|(index, _)| *index);
    }

    points
}

fn collect_trail_magnitudes_from(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    component: re_sdk_types::ComponentIdentifier,
    first_point_index: Option<TrailTimeIndex>,
) -> Vec<TimedTrailValue<f64>> {
    let mut magnitudes = Vec::new();
    let mut previous_index = None;
    let mut needs_sort = false;

    for ((time, row_id), magnitude_batch) in results.iter_optional(component).slice::<f64>() {
        for (sample_index, magnitude) in magnitude_batch.iter().copied().enumerate() {
            let index = (time, row_id, sample_index);
            if first_point_index.is_none_or(|first_index| index >= first_index) {
                if previous_index.is_some_and(|previous| previous > index) {
                    needs_sort = true;
                }
                previous_index = Some(index);
                magnitudes.push((index, magnitude));
            }
        }
    }

    if needs_sort {
        magnitudes.sort_by_key(|(index, _)| *index);
    }

    magnitudes
}

fn latest_magnitude_range(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    component: re_sdk_types::ComponentIdentifier,
) -> Trail3DMagnitudeRange {
    // TODO(#azk): `latest_color`, `latest_magnitude_range`, `latest_radius`, and
    // `effective_trail_length` all hand-roll latest-at field scanning. Keep this explicit for now,
    // but consider a small direct-slice helper if another visualizer needs the same pattern.
    let mut latest = None;

    let range_results = results.iter_optional(component);
    for ((index, modes), (_, mins), (_, maxes)) in range_results
        .slice_from_struct_field::<u8>("mode")
        .zip(range_results.slice_from_struct_field::<f64>("min"))
        .zip(range_results.slice_from_struct_field::<f64>("max"))
        .map(|((mode, min), max)| (mode, min, max))
    {
        for ((mode, min), max) in modes.iter().copied().zip(mins).zip(maxes) {
            if latest.is_none_or(|(current_index, _)| current_index <= index) {
                latest = Some((
                    index,
                    Trail3DMagnitudeRange::from(re_sdk_types::datatypes::Trail3DMagnitudeRange {
                        mode,
                        min: *min,
                        max: *max,
                    }),
                ));
            }
        }
    }

    latest.map(|(_, range)| range).unwrap_or_default()
}

fn latest_color(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    component: re_sdk_types::ComponentIdentifier,
) -> Trail3DColor {
    let mut latest = None;

    let color_results = results.iter_optional(component);
    for ((index, modes), (_, colors), (_, colormaps)) in color_results
        .slice_from_struct_field::<u8>("mode")
        .zip(color_results.slice_from_struct_field::<u32>("color"))
        .zip(color_results.slice_from_struct_field::<u8>("colormap"))
        .map(|((mode, color), colormap)| (mode, color, colormap))
    {
        for ((mode, color), colormap) in modes.iter().copied().zip(colors).zip(colormaps) {
            if latest.is_none_or(|(current_index, _)| current_index <= index) {
                latest = Some((
                    index,
                    Trail3DColor::from(re_sdk_types::datatypes::Trail3DColor {
                        mode,
                        color: *color,
                        colormap: *colormap,
                    }),
                ));
            }
        }
    }

    latest
        .map(|(_, color)| color)
        .unwrap_or_else(default_trail3d_color_component)
}

fn latest_radius(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    component: re_sdk_types::ComponentIdentifier,
) -> Option<re_sdk_types::components::Radius> {
    let mut latest = None;

    for (index, radii) in results.iter_optional(component).slice::<f32>() {
        for radius in radii.iter().copied() {
            if latest.is_none_or(|(current_index, _)| current_index <= index) {
                latest = Some((index, re_sdk_types::components::Radius(radius.into())));
            }
        }
    }

    latest.map(|(_, radius)| radius)
}

fn effective_trail_length(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    entity_path: &re_log_types::EntityPath,
    component: re_sdk_types::ComponentIdentifier,
) -> Trail3DLength {
    let mut latest = None;

    let length_results = results.iter_optional(component);
    for ((index, seconds), (_, ticks)) in length_results
        .slice_from_struct_field::<f64>("seconds")
        .zip(length_results.slice_from_struct_field::<u64>("ticks"))
    {
        for (seconds, ticks) in seconds.iter().copied().zip(ticks.iter().copied()) {
            if latest.is_none_or(|(current_index, _)| current_index <= index) {
                latest = Some((index, Trail3DLength::new(seconds, ticks)));
            }
        }
    }

    let logged_length = latest
        .map(|(_, length)| length)
        .unwrap_or_else(default_trail3d_length_component);

    let clamped_length = clamp_trail_length_component(logged_length);
    if clamped_length != logged_length {
        results.report_unspecified_source(
            VisualizerReportSeverity::Warning,
            format!(
                "Trail3D length for {entity_path} was clamped from {:.3}s/{} ticks to {:.3}s/{} ticks.",
                logged_length.seconds,
                logged_length.ticks,
                clamped_length.seconds,
                clamped_length.ticks,
            ),
        );
    }

    clamped_length
}

fn clamp_trail_length_component(length: Trail3DLength) -> Trail3DLength {
    let seconds = if length.seconds.is_finite() {
        length
            .seconds
            .clamp(MIN_TRAIL_LENGTH_SECONDS, MAX_TRAIL_LENGTH_SECONDS)
    } else {
        DEFAULT_TRAIL_LENGTH_SECONDS
    };
    let ticks = length
        .ticks
        .clamp(MIN_TRAIL_LENGTH_TICKS, MAX_TRAIL_LENGTH_TICKS);
    Trail3DLength::new(seconds, ticks)
}

fn trail_history_ticks(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    entity_path: &re_log_types::EntityPath,
    time_type: TimeType,
    length: Trail3DLength,
) -> i64 {
    match time_type {
        TimeType::Sequence => length.ticks.min(i64::MAX as u64) as i64,
        TimeType::DurationNs | TimeType::TimestampNs => {
            let nanos = (length.seconds * 1e9).round();
            if !nanos.is_finite() || nanos <= 0.0 {
                results.report_unspecified_source(
                    VisualizerReportSeverity::Warning,
                    format!(
                        "Invalid Trail3D temporal length for {entity_path}: {:.3}s. Falling back to {DEFAULT_TRAIL_LENGTH_SECONDS:.3}s.",
                        length.seconds
                    ),
                );
                re_log_types::TimeInt::from_secs(DEFAULT_TRAIL_LENGTH_SECONDS).as_i64()
            } else if nanos >= i64::MAX as f64 {
                i64::MAX
            } else {
                nanos as i64
            }
        }
    }
}

fn align_magnitude_trail_samples(
    points: Vec<TimedTrailValue<glam::Vec3>>,
    magnitudes: &[TimedTrailValue<f64>],
    missing_magnitude: f64,
) -> Vec<MagnitudeTrailSample> {
    let mut magnitude_index = 0;
    let mut samples = Vec::with_capacity(points.len());

    for (point_index, point) in points {
        while magnitude_index < magnitudes.len() && magnitudes[magnitude_index].0 < point_index {
            magnitude_index += 1;
        }

        let magnitude =
            if magnitude_index < magnitudes.len() && magnitudes[magnitude_index].0 == point_index {
                magnitudes[magnitude_index].1
            } else {
                missing_magnitude
            };

        samples.push(MagnitudeTrailSample { point, magnitude });
    }

    samples
}

fn color_mapping_for_magnitudes(
    results: &re_view::VisualizerInstructionQueryResults<'_>,
    entity_path: &re_log_types::EntityPath,
    magnitudes: &[TimedTrailValue<f64>],
    magnitude_range: Trail3DMagnitudeRange,
    colormap: re_sdk_types::components::Colormap,
) -> Option<TrailColorMapping> {
    let auto_max = magnitudes
        .iter()
        .map(|(_, magnitude)| *magnitude)
        .filter(|magnitude| magnitude.is_finite())
        .fold(0.0_f64, f64::max);
    let auto_range = [0.0, auto_max.max(f64::EPSILON)];

    if magnitude_range.is_fixed()
        && let [min, max] = magnitude_range.fixed_range()
        && !(min.is_finite() && max.is_finite() && min < max)
    {
        results.report_unspecified_source(
            VisualizerReportSeverity::Error,
            format!(
                "Invalid Trail3D magnitude_range for {entity_path}: [{min}, {max}]. Expected finite min < max. Trail will not be rendered."
            ),
        );
        return None;
    }

    let fixed_range = magnitude_range.fixed_range();
    let range = if magnitude_range.is_auto() {
        auto_range
    } else {
        fixed_range
    };

    Some(TrailColorMapping {
        range,
        colormap: re_viewer_context::gpu_bridge::colormap_to_re_renderer(colormap),
    })
}

fn add_trail_segments(
    line_builder: &mut re_renderer::LineDrawableBuilder<'_>,
    trail: TrailRenderData,
    outline_mask_ids: re_renderer::OutlineMaskPreference,
) {
    match trail {
        TrailRenderData::Solid(trail) => {
            line_builder
                .batch(trail.entity_path.to_string())
                .world_from_obj(trail.world_from_obj)
                .outline_mask_ids(outline_mask_ids)
                .picking_object_id(re_renderer::PickingLayerObjectId(
                    trail.entity_path.hash64(),
                ))
                .add_strip(trail.points.into_iter())
                .flags(
                    re_renderer::renderer::LineStripFlags::STRIP_FLAGS_OUTWARD_EXTENDING_ROUND_CAPS,
                )
                .color(trail.color)
                .radius(trail.radius);
        }
        TrailRenderData::Magnitude(trail) => {
            let mut line_batch = line_builder
                .batch(trail.entity_path.to_string())
                .world_from_obj(trail.world_from_obj)
                .outline_mask_ids(outline_mask_ids)
                .picking_object_id(re_renderer::PickingLayerObjectId(
                    trail.entity_path.hash64(),
                ));

            // TODO(#azk): Add per-vertex/per-segment color support to the line renderer so magnitude
            // trails can be submitted as one strip instead of one strip per colored segment.
            for segment in trail.samples.windows(2) {
                let [start, end] = segment else {
                    continue;
                };

                let magnitude = (start.magnitude + end.magnitude) * 0.5;
                let color = colormapped_magnitude_color(magnitude, trail.color_mapping);

                line_batch
                    .add_strip([start.point, end.point].into_iter())
                    .flags(
                        re_renderer::renderer::LineStripFlags::STRIP_FLAGS_OUTWARD_EXTENDING_ROUND_CAPS,
                    )
                    .color(color)
                    .radius(trail.radius);
            }
        }
    }
}

fn default_trail3d_radius_size() -> re_renderer::Size {
    re_renderer::Size(*default_trail3d_radius_component().0)
}

fn process_radius(
    entity_path: &re_log_types::EntityPath,
    radius: re_sdk_types::components::Radius,
) -> re_renderer::Size {
    if radius.0.is_infinite() {
        re_log::warn_once!("Found infinite Trail3D radius in entity {entity_path}");
    } else if radius.0.is_nan() {
        re_log::warn_once!("Found NaN Trail3D radius in entity {entity_path}");
    }

    re_renderer::Size(*radius.0)
}

fn colormapped_magnitude_color(
    magnitude: f64,
    color_mapping: TrailColorMapping,
) -> re_renderer::Color32 {
    let [min, max] = color_mapping.range;
    let normalized = ((magnitude - min) / (max - min)) as f32;
    let [r, g, b, a] =
        re_renderer::colormap_srgba(color_mapping.colormap, normalized.clamp(0.0, 1.0));
    re_renderer::Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn sdk_color_to_renderer(color: u32) -> re_renderer::Color32 {
    let [r, g, b, a] = color.to_be_bytes();
    re_renderer::Color32::from_rgba_unmultiplied(r, g, b, a)
}
