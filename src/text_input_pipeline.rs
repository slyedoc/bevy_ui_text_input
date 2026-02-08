use crate::TextInputBuffer;
use crate::TextInputGlyph;
use crate::TextInputLayoutInfo;
use crate::TextInputNode;
use crate::TextInputPrompt;
use crate::TextInputPromptLayoutInfo;
use bevy::asset::AssetEvent;
use bevy::asset::AssetId;
use bevy::asset::Assets;
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::message::MessageReader;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::Query;
use bevy::ecs::system::Res;
use bevy::ecs::system::ResMut;
use bevy::ecs::world::Ref;
use bevy::image::Image;
use bevy::image::TextureAtlasLayout;
use bevy::math::Rect;
use bevy::math::UVec2;
use bevy::math::Vec2;
use bevy::platform::collections::HashMap;
use bevy::text::Font;
use bevy::text::FontAtlas;
use bevy::text::FontAtlasKey;
use bevy::text::FontSmoothing;
use bevy::text::FontSource;
use bevy::text::Justify;
use bevy::text::LineBreak;
use bevy::text::TextBounds;
use bevy::text::TextError;
use bevy::text::RemSize;
use bevy::text::TextFont;
use bevy::text::add_glyph_to_atlas;
use bevy::text::get_glyph_atlas_info;

fn justify_to_align(justify: Justify) -> cosmic_text::Align {
    match justify {
        Justify::Left => cosmic_text::Align::Left,
        Justify::Center => cosmic_text::Align::Center,
        Justify::Right => cosmic_text::Align::Right,
        Justify::Justified => cosmic_text::Align::Justified,
    }
}
use bevy::ui::ComputedNode;
use bevy::ui::ComputedUiRenderTargetInfo;
use cosmic_text;
use cosmic_text::Buffer;
use cosmic_text::Edit;
use cosmic_text::Metrics;
use std::sync::Arc;

#[derive(Resource)]
pub struct TextInputPipeline {
    pub(crate) handle_to_font_id_map: HashMap<AssetId<Font>, (cosmic_text::fontdb::ID, Arc<str>)>,
    pub font_system: cosmic_text::FontSystem,
    pub(crate) swash_cache: cosmic_text::SwashCache,
    pub(crate) font_atlas_sets: HashMap<FontAtlasKey, Vec<FontAtlas>>,
}

impl Default for TextInputPipeline {
    fn default() -> Self {
        let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
        let db = cosmic_text::fontdb::Database::new();
        Self {
            handle_to_font_id_map: Default::default(),
            font_system: cosmic_text::FontSystem::new_with_locale_and_db(locale, db),
            swash_cache: cosmic_text::SwashCache::new(),
            font_atlas_sets: Default::default(),
        }
    }
}

#[derive(Clone)]
struct FontFaceInfo {
    stretch: cosmic_text::fontdb::Stretch,
    style: cosmic_text::fontdb::Style,
    weight: cosmic_text::fontdb::Weight,
    family_name: Arc<str>,
}

fn load_font_to_fontdb(
    text_font: &TextFont,
    font_system: &mut cosmic_text::FontSystem,
    map_handle_to_font_id: &mut HashMap<AssetId<Font>, (cosmic_text::fontdb::ID, Arc<str>)>,
    fonts: &Assets<Font>,
) -> Option<FontFaceInfo> {
    match &text_font.font {
        FontSource::Handle(font_handle) => {
            let (face_id, family_name) = map_handle_to_font_id
                .entry(font_handle.id())
                .or_insert_with(|| {
                    let font = fonts.get(font_handle.id()).expect(
                        "Tried getting a font that was not available, probably due to not being loaded yet",
                    );
                    let data = Arc::clone(&font.data);
                    let ids = font_system
                        .db_mut()
                        .load_font_source(cosmic_text::fontdb::Source::Binary(data));

                    // TODO: it is assumed this is the right font face
                    let face_id = *ids.last().unwrap();
                    let face = font_system.db().face(face_id).unwrap();
                    let family_name = Arc::from(face.families[0].0.as_str());

                    (face_id, family_name)
                });
            let face = font_system.db().face(*face_id).unwrap();

            Some(FontFaceInfo {
                stretch: face.stretch,
                style: face.style,
                weight: face.weight,
                family_name: family_name.clone(),
            })
        }
        FontSource::Family(family) => {
            // For family-based fonts, look up the font in the font system database
            let family_name = Arc::from(family.as_str());
            // Query the font system for a matching font face
            let query = cosmic_text::fontdb::Query {
                families: &[cosmic_text::fontdb::Family::Name(family.as_str())],
                weight: cosmic_text::fontdb::Weight(text_font.weight.0),
                stretch: cosmic_text::fontdb::Stretch::Normal,
                style: match text_font.style {
                    bevy::text::FontStyle::Normal => cosmic_text::fontdb::Style::Normal,
                    bevy::text::FontStyle::Italic => cosmic_text::fontdb::Style::Italic,
                    bevy::text::FontStyle::Oblique => cosmic_text::fontdb::Style::Oblique,
                },
            };
            if let Some(face_id) = font_system.db().query(&query) {
                let face = font_system.db().face(face_id).unwrap();
                Some(FontFaceInfo {
                    stretch: face.stretch,
                    style: face.style,
                    weight: face.weight,
                    family_name,
                })
            } else {
                None
            }
        }
        // Generic font families (Serif, SansSerif, Cursive, Fantasy, Monospace)
        font_source => {
            let (fontdb_family, family_name): (cosmic_text::fontdb::Family, Arc<str>) =
                match font_source {
                    FontSource::Serif => (cosmic_text::fontdb::Family::Serif, Arc::from("serif")),
                    FontSource::SansSerif => {
                        (cosmic_text::fontdb::Family::SansSerif, Arc::from("sans-serif"))
                    }
                    FontSource::Cursive => {
                        (cosmic_text::fontdb::Family::Cursive, Arc::from("cursive"))
                    }
                    FontSource::Fantasy => {
                        (cosmic_text::fontdb::Family::Fantasy, Arc::from("fantasy"))
                    }
                    FontSource::Monospace => {
                        (cosmic_text::fontdb::Family::Monospace, Arc::from("monospace"))
                    }
                    _ => unreachable!(),
                };
            let query = cosmic_text::fontdb::Query {
                families: &[fontdb_family],
                weight: cosmic_text::fontdb::Weight(text_font.weight.0),
                stretch: cosmic_text::fontdb::Stretch::Normal,
                style: match text_font.style {
                    bevy::text::FontStyle::Normal => cosmic_text::fontdb::Style::Normal,
                    bevy::text::FontStyle::Italic => cosmic_text::fontdb::Style::Italic,
                    bevy::text::FontStyle::Oblique => cosmic_text::fontdb::Style::Oblique,
                },
            };
            if let Some(face_id) = font_system.db().query(&query) {
                let face = font_system.db().face(face_id).unwrap();
                Some(FontFaceInfo {
                    stretch: face.stretch,
                    style: face.style,
                    weight: face.weight,
                    family_name,
                })
            } else {
                None
            }
        }
    }
}

fn buffer_dimensions(buffer: &cosmic_text::Buffer) -> Vec2 {
    let (width, height) = buffer
        .layout_runs()
        .map(|run| (run.line_w, run.line_height))
        .reduce(|(w1, h1), (w2, h2)| (w1.max(w2), h1 + h2))
        .unwrap_or((0.0, 0.0));

    Vec2::new(width, height).ceil()
}

pub fn text_input_system(
    mut textures: ResMut<Assets<Image>>,
    fonts: Res<Assets<Font>>,
    mut texture_atlases: ResMut<Assets<TextureAtlasLayout>>,
    mut text_input_pipeline: ResMut<TextInputPipeline>,
    rem_size: Res<RemSize>,
    mut text_query: Query<(
        Ref<ComputedNode>,
        Ref<TextFont>,
        &mut TextInputLayoutInfo,
        &mut TextInputBuffer,
        Ref<TextInputNode>,
        Ref<ComputedUiRenderTargetInfo>,
    )>,
) {
    for (node, text_font, text_input_layout_info, mut editor, input, render_target) in text_query.iter_mut() {
        let layout_info = text_input_layout_info.into_inner();
        if editor.needs_update || text_font.is_changed() || node.is_changed() || input.is_changed()
        {
            let bounds = TextBounds {
                width: Some(node.size().x),
                height: Some(node.size().y),
            };

            let logical_viewport_size = render_target.logical_size();
            let font_size = text_font.font_size.eval(logical_viewport_size, rem_size.0);
            // LineHeight::default() is RelativeToFont(1.2)
            let line_height = 1.2 * font_size;

            let result = editor.editor.with_buffer_mut(|buffer| {
                let TextInputPipeline {
                    font_system,
                    handle_to_font_id_map: map_handle_to_font_id,
                    ..
                } = &mut *text_input_pipeline;
                // Check if font is available (for Handle-based fonts)
                if let FontSource::Handle(ref handle) = text_font.font {
                    if !fonts.contains(handle.id()) {
                        return Err(TextError::NoSuchFont);
                    }
                }

                let Some(face_info) =
                    load_font_to_fontdb(&text_font, font_system, map_handle_to_font_id, &fonts)
                else {
                    return Err(TextError::NoSuchFont);
                };

                let mut metrics = Metrics::new(font_size, line_height)
                    .scale(node.inverse_scale_factor().recip());

                metrics.font_size = metrics.font_size.max(0.000001);
                metrics.line_height = metrics.line_height.max(0.000001);

                buffer.set_metrics_and_size(font_system, metrics, bounds.width, bounds.height);

                buffer.set_wrap(font_system, input.mode.wrap());

                let attrs = cosmic_text::Attrs::new()
                    .metadata(0)
                    .family(cosmic_text::Family::Name(&face_info.family_name))
                    .stretch(face_info.stretch)
                    .style(face_info.style)
                    .weight(face_info.weight)
                    .metrics(metrics);

                let text = crate::get_text(buffer);
                let align = Some(justify_to_align(input.justification));
                buffer.set_text(font_system, &text, &attrs, cosmic_text::Shaping::Advanced, align);

                Ok(())
            });

            if result.is_ok() {
                editor.needs_update = false;
                editor.editor.set_redraw(true);
            } else {
                editor.needs_update = true;
                continue;
            }
        }

        editor
            .editor
            .shape_as_needed(&mut text_input_pipeline.font_system, false);

        let selection = editor.editor.selection_bounds();
        let TextInputBuffer {
            editor,
            selection_rects,
            ..
        } = &mut *editor;

        if editor.redraw() {
            layout_info.glyphs.clear();
            selection_rects.clear();

            let result = editor.with_buffer_mut(|buffer| {
                let box_size = buffer_dimensions(buffer);
                let result = buffer.layout_runs().try_for_each(|run| {
                    if let Some(selection) = selection
                        && let Some((x0, w)) = run.highlight(selection.0, selection.1)
                    {
                        let y0 = run.line_top;
                        let y1 = y0 + run.line_height;
                        let x1 = x0 + w;
                        let r = Rect::new(x0, y0, x1, y1);
                        selection_rects.push(r);
                    }

                    run.glyphs
                        .iter()
                        .map(move |layout_glyph| (layout_glyph, run.line_y, run.line_i))
                        .try_for_each(|(layout_glyph, line_y, line_i)| {
                            let mut temp_glyph;
                            let span_index = layout_glyph.metadata;
                            let font_smoothing = text_font.font_smoothing;

                            let layout_glyph = if font_smoothing == FontSmoothing::None {
                                // If font smoothing is disabled, round the glyph positions and sizes,
                                // effectively discarding all subpixel layout.
                                temp_glyph = layout_glyph.clone();
                                temp_glyph.x = temp_glyph.x.round();
                                temp_glyph.y = temp_glyph.y.round();
                                temp_glyph.w = temp_glyph.w.round();
                                temp_glyph.x_offset = temp_glyph.x_offset.round();
                                temp_glyph.y_offset = temp_glyph.y_offset.round();
                                temp_glyph.line_height_opt =
                                    temp_glyph.line_height_opt.map(f32::round);

                                &temp_glyph
                            } else {
                                layout_glyph
                            };

                            let physical_glyph = layout_glyph.physical((0., 0.), 1.);

                            let TextInputPipeline {
                                font_system,
                                swash_cache,
                                font_atlas_sets,
                                ..
                            } = &mut *text_input_pipeline;

                            let font_atlases = font_atlas_sets
                                .entry(FontAtlasKey {
                                    id: physical_glyph.cache_key.font_id,
                                    font_size_bits: physical_glyph.cache_key.font_size_bits,
                                    font_smoothing,
                                })
                                .or_default();

                            let atlas_info = get_glyph_atlas_info(font_atlases, physical_glyph.cache_key)
                                .map(Ok)
                                .unwrap_or_else(|| {
                                    add_glyph_to_atlas(
                                        font_atlases,
                                        &mut texture_atlases,
                                        &mut textures,
                                        font_system,
                                        swash_cache,
                                        layout_glyph,
                                        font_smoothing,
                                    )
                                })?;

                            let texture_atlas =
                                texture_atlases.get(atlas_info.texture_atlas).unwrap();
                            let location = atlas_info.location;
                            let glyph_rect = texture_atlas.textures[location.glyph_index];
                            let left = location.offset.x as f32;
                            let top = location.offset.y as f32;
                            let glyph_size = UVec2::new(glyph_rect.width(), glyph_rect.height());

                            // offset by half the size because the origin is center
                            let x = glyph_size.x as f32 / 2.0 + left + physical_glyph.x as f32;
                            let y = line_y.round() + physical_glyph.y as f32 - top
                                + glyph_size.y as f32 / 2.0;

                            let position = Vec2::new(x, y);

                            let pos_glyph = TextInputGlyph {
                                position,
                                size: glyph_size.as_vec2(),
                                atlas_info,
                                span_index,
                                byte_index: layout_glyph.start,
                                byte_length: layout_glyph.end - layout_glyph.start,
                                line_index: line_i,
                            };
                            layout_info.glyphs.push(pos_glyph);
                            Ok(())
                        })
                });

                // Check result.
                result?;

                layout_info.size = box_size;
                Ok(())
            });

            match result {
                Err(TextError::NoSuchFont) => {
                    // There was an error processing the text layout, try again next frame
                }
                Err(e) => {
                    panic!("Fatal error when processing text: {e}.");
                }
                Ok(()) => {
                    layout_info.size.x *= node.inverse_scale_factor();
                    layout_info.size.y *= node.inverse_scale_factor();
                    editor.set_redraw(false);
                }
            }
        }
    }
}

pub fn text_input_prompt_system(
    mut textures: ResMut<Assets<Image>>,
    fonts: Res<Assets<Font>>,
    mut texture_atlases: ResMut<Assets<TextureAtlasLayout>>,
    mut text_input_pipeline: ResMut<TextInputPipeline>,
    rem_size: Res<RemSize>,
    mut text_query: Query<(
        Ref<ComputedNode>,
        Ref<TextFont>,
        &mut TextInputPromptLayoutInfo,
        &mut TextInputBuffer,
        Ref<TextInputNode>,
        Ref<TextInputPrompt>,
        Ref<ComputedUiRenderTargetInfo>,
    )>,
) {
    for (node, text_font, text_input_layout_info, mut editor, input, prompt, render_target) in
        text_query.iter_mut()
    {
        let layout_info = text_input_layout_info.into_inner();
        if prompt.is_changed()
            || input.is_changed()
            || editor.prompt_buffer.is_none()
            || layout_info.glyphs.is_empty()
            || text_font.is_changed() && prompt.font.is_none()
            || node.is_changed()
        {
            layout_info.glyphs.clear();

            if prompt.text.is_empty() {
                editor.prompt_buffer = None;
                continue;
            }

            let TextInputPipeline {
                font_system,
                handle_to_font_id_map: map_handle_to_font_id,
                ..
            } = &mut *text_input_pipeline;
            // Check if font is available (for Handle-based fonts)
            if let FontSource::Handle(ref handle) = text_font.font {
                if !fonts.contains(handle.id()) {
                    editor.prompt_buffer = None;
                    continue;
                }
            }

            let font = prompt.font.as_ref().unwrap_or(text_font.as_ref());

            let logical_viewport_size = render_target.logical_size();
            let font_size = font.font_size.eval(logical_viewport_size, rem_size.0);
            // LineHeight::default() is RelativeToFont(1.2)
            let line_height = 1.2 * font_size;

            let metrics = Metrics::new(font_size, line_height)
                .scale(node.inverse_scale_factor().recip());

            if metrics.font_size <= 0. || metrics.line_height <= 0. {
                editor.prompt_buffer = None;
                continue;
            }

            let buffer = editor
                .prompt_buffer
                .get_or_insert(Buffer::new(font_system, metrics));

            let linebreak = LineBreak::WordBoundary;
            let bounds = TextBounds {
                width: Some(node.size().x),
                height: Some(node.size().y),
            };

            let Some(face_info) = load_font_to_fontdb(font, font_system, map_handle_to_font_id, &fonts) else {
                editor.prompt_buffer = None;
                continue;
            };

            buffer.set_size(font_system, bounds.width, bounds.height);

            buffer.set_wrap(
                font_system,
                match linebreak {
                    LineBreak::WordBoundary => cosmic_text::Wrap::Word,
                    LineBreak::AnyCharacter => cosmic_text::Wrap::Glyph,
                    LineBreak::WordOrCharacter => cosmic_text::Wrap::WordOrGlyph,
                    LineBreak::NoWrap => cosmic_text::Wrap::None,
                },
            );

            let attrs = cosmic_text::Attrs::new()
                .metadata(0)
                .family(cosmic_text::Family::Name(&face_info.family_name))
                .stretch(face_info.stretch)
                .style(face_info.style)
                .weight(face_info.weight)
                .metrics(metrics);

            let align = Some(justify_to_align(input.justification));
            buffer.set_text(
                font_system,
                &prompt.text,
                &attrs,
                cosmic_text::Shaping::Advanced,
                align,
            );

            buffer.shape_until_scroll(font_system, false);

            let box_size = buffer_dimensions(buffer);
            let result = buffer.layout_runs().try_for_each(|run| {
                run.glyphs
                    .iter()
                    .map(move |layout_glyph| (layout_glyph, run.line_y, run.line_i))
                    .try_for_each(|(layout_glyph, line_y, line_i)| {
                        let mut temp_glyph;
                        let span_index = layout_glyph.metadata;
                        let font_smoothing = text_font.font_smoothing;

                        let layout_glyph = if font_smoothing == FontSmoothing::None {
                            // If font smoothing is disabled, round the glyph positions and sizes,
                            // effectively discarding all subpixel layout.
                            temp_glyph = layout_glyph.clone();
                            temp_glyph.x = temp_glyph.x.round();
                            temp_glyph.y = temp_glyph.y.round();
                            temp_glyph.w = temp_glyph.w.round();
                            temp_glyph.x_offset = temp_glyph.x_offset.round();
                            temp_glyph.y_offset = temp_glyph.y_offset.round();
                            temp_glyph.line_height_opt = temp_glyph.line_height_opt.map(f32::round);

                            &temp_glyph
                        } else {
                            layout_glyph
                        };

                        let physical_glyph = layout_glyph.physical((0., 0.), 1.);

                        let TextInputPipeline {
                            font_system,
                            swash_cache,
                            font_atlas_sets,
                            ..
                        } = &mut *text_input_pipeline;

                        let font_atlases = font_atlas_sets
                            .entry(FontAtlasKey {
                                id: physical_glyph.cache_key.font_id,
                                font_size_bits: physical_glyph.cache_key.font_size_bits,
                                font_smoothing,
                            })
                            .or_default();

                        let atlas_info = get_glyph_atlas_info(font_atlases, physical_glyph.cache_key)
                            .map(Ok)
                            .unwrap_or_else(|| {
                                add_glyph_to_atlas(
                                    font_atlases,
                                    &mut texture_atlases,
                                    &mut textures,
                                    font_system,
                                    swash_cache,
                                    layout_glyph,
                                    font_smoothing,
                                )
                            })?;

                        let texture_atlas = texture_atlases.get(atlas_info.texture_atlas).unwrap();
                        let location = atlas_info.location;
                        let glyph_rect = texture_atlas.textures[location.glyph_index];
                        let left = location.offset.x as f32;
                        let top = location.offset.y as f32;
                        let glyph_size = UVec2::new(glyph_rect.width(), glyph_rect.height());

                        // offset by half the size because the origin is center
                        let x = glyph_size.x as f32 / 2.0 + left + physical_glyph.x as f32;
                        let y = line_y.round() + physical_glyph.y as f32 - top
                            + glyph_size.y as f32 / 2.0;

                        let position = Vec2::new(x, y);

                        let pos_glyph = TextInputGlyph {
                            position,
                            size: glyph_size.as_vec2(),
                            atlas_info,
                            span_index,
                            byte_index: layout_glyph.start,
                            byte_length: layout_glyph.end - layout_glyph.start,
                            line_index: line_i,
                        };
                        layout_info.glyphs.push(pos_glyph);
                        Ok(())
                    })
            });

            layout_info.size = box_size;

            match result {
                Err(TextError::NoSuchFont) => {
                    editor.prompt_buffer = None;
                    // There was an error processing the text layout, try again next frame
                }
                Err(e) => {
                    panic!("Fatal error when processing text: {e}.");
                }
                Ok(()) => {
                    layout_info.size.x *= node.inverse_scale_factor();
                    layout_info.size.y *= node.inverse_scale_factor();
                }
            }
        }
    }
}

pub fn remove_dropped_font_atlas_sets_from_text_input_pipeline(
    mut _text_input_pipeline: ResMut<TextInputPipeline>,
    mut font_events: MessageReader<AssetEvent<Font>>,
) {
    // Note: In bevy main, FontAtlasKey uses cosmic_text::fontdb::ID rather than AssetId<Font>.
    // When a font is removed, we no longer have access to the Font asset (and its fontdb IDs),
    // so we can't easily clean up the font atlas sets.
    // Bevy's own text pipeline doesn't perform this cleanup either.
    for _event in font_events.read() {
        // Drain the event reader to avoid warning about unused events
    }
}
