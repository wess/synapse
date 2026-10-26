use gpui::RenderImage;
use image::{Frame, Rgba, RgbaImage, imageops};
use std::sync::Arc;
use synapsecore::{
    plugins::{Plugin, colorvalue},
    presence::State,
};

include!(concat!(env!("OUT_DIR"), "/scene.rs"));
pub const WIDTH: u32 = 144;
pub const HEIGHT: u32 = 104;

pub struct Art {
    pub characters: Vec<RgbaImage>,
    pub background: Option<RgbaImage>,
}

pub fn load(plugin: &Plugin) -> anyhow::Result<Art> {
    let custom = |name: &Option<String>| -> anyhow::Result<Option<RgbaImage>> {
        match (&plugin.root, name) {
            (Some(root), Some(name)) => {
                let path = synapsecore::plugins::asset(root, name)?;
                let mut reader = image::ImageReader::open(path)?;
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(4096);
                limits.max_image_height = Some(4096);
                limits.max_alloc = Some(64 * 1024 * 1024);
                reader.limits(limits);
                Ok(Some(reader.decode()?.into_rgba8()))
            }
            _ => Ok(None),
        }
    };
    let characters = match custom(&plugin.manifest.scene.characters)? {
        Some(sheet) => vec![sheet],
        None => SPRITES
            .iter()
            .map(|bytes| image::load_from_memory(bytes).map(|image| image.into_rgba8()))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let scene = &plugin.manifest.scene;
    for sheet in &characters {
        anyhow::ensure!(
            sheet.width() >= scene.framewidth * 12 && sheet.height() >= scene.frameheight * 8,
            "character sheet must have 12 columns and 8 rows of frames"
        );
    }
    let background = custom(&scene.background)?;
    if let Some(background) = &background {
        anyhow::ensure!(
            background.dimensions() == (WIDTH, HEIGHT),
            "room background must be {WIDTH} × {HEIGHT} pixels"
        );
    }
    Ok(Art {
        characters,
        background,
    })
}

fn rect(image: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
    for py in y..(y + h).min(image.height()) {
        for px in x..(x + w).min(image.width()) {
            image.put_pixel(px, py, Rgba(color));
        }
    }
}
fn shade(mut color: [u8; 4], delta: i16) -> [u8; 4] {
    for channel in &mut color[..3] {
        *channel = (*channel as i16 + delta).clamp(0, 255) as u8;
    }
    color
}

pub fn room(
    plugin: &Plugin,
    art: &Art,
    appearance: usize,
    state: State,
    tick: u64,
    inhabited: bool,
) -> Arc<RenderImage> {
    let scene = &plugin.manifest.scene;
    let floor = colorvalue(&scene.floor).unwrap_or([180, 138, 96, 255]);
    let wall = colorvalue(&scene.wall).unwrap_or([82, 89, 101, 255]);
    let wood = colorvalue(&scene.wood).unwrap_or([115, 81, 61, 255]);
    let accent = colorvalue(&scene.accent).unwrap_or([233, 180, 92, 255]);
    let mut image = art.background.clone().unwrap_or_else(|| {
        let mut image = RgbaImage::from_pixel(WIDTH, HEIGHT, Rgba([31, 30, 38, 255]));
        rect(&mut image, 4, 4, 136, 91, wall);
        // Stone courses, timber cap, and alternating wooden floorboards.
        for row in 0..4 {
            for column in 0..9 {
                let x = 4 + column * 17 + (row % 2) * 8;
                rect(
                    &mut image,
                    x,
                    6 + row * 7,
                    16,
                    6,
                    shade(wall, ((column + row) % 3) as i16 * 5),
                );
            }
        }
        rect(&mut image, 4, 34, 136, 61, floor);
        for row in 0..8 {
            for column in 0..7 {
                let x = 4 + column * 24;
                rect(
                    &mut image,
                    x,
                    35 + row * 8,
                    23,
                    7,
                    shade(floor, ((row + column) % 3) as i16 * 4 - 8),
                );
                rect(&mut image, x + 4, 37 + row * 8, 10, 1, shade(floor, -12));
            }
        }
        rect(&mut image, 4, 31, 136, 4, shade(wood, -20));
        rect(&mut image, 2, 2, 140, 4, shade(wood, 12));
        for x in [2, 136] {
            rect(&mut image, x, 6, 6, 89, wood);
            rect(&mut image, x + 1, 6, 1, 89, shade(wood, 20));
        }
        rect(&mut image, 2, 95, 140, 6, shade(wood, -16));
        // Window and a small herb on the sill.
        rect(&mut image, 21, 12, 26, 23, wood);
        rect(&mut image, 24, 14, 20, 17, [104, 151, 164, 255]);
        rect(&mut image, 33, 14, 2, 18, wood);
        rect(&mut image, 24, 22, 20, 2, wood);
        rect(&mut image, 18, 34, 32, 3, shade(wood, 15));
        rect(&mut image, 43, 29, 5, 6, [154, 93, 70, 255]);
        rect(&mut image, 44, 23, 3, 7, [94, 127, 77, 255]);
        rect(&mut image, 41, 25, 8, 3, [114, 145, 84, 255]);
        // Fireplace and chimney.
        rect(&mut image, 104, 7, 19, 26, shade(wall, 20));
        rect(&mut image, 99, 27, 29, 21, shade(wall, 30));
        rect(&mut image, 105, 31, 16, 15, [38, 31, 36, 255]);
        rect(&mut image, 97, 46, 33, 4, shade(wall, -15));
        rect(&mut image, 102, 42, 22, 3, shade(wood, -10));
        // Rug, workbench, tools, and a bookshelf.
        rect(&mut image, 43, 62, 55, 26, [114, 67, 69, 255]);
        rect(&mut image, 46, 65, 49, 20, [155, 95, 83, 255]);
        rect(&mut image, 48, 67, 45, 1, accent);
        rect(&mut image, 48, 82, 45, 1, accent);
        rect(&mut image, 54, 46, 4, 13, shade(wood, -22));
        rect(&mut image, 86, 46, 4, 13, shade(wood, -22));
        rect(&mut image, 50, 43, 44, 8, shade(wood, -20));
        rect(&mut image, 49, 39, 46, 8, wood);
        rect(&mut image, 50, 39, 44, 2, shade(wood, 25));
        rect(&mut image, 56, 36, 13, 4, [141, 151, 154, 255]);
        rect(&mut image, 61, 33, 4, 5, [164, 170, 165, 255]);
        rect(&mut image, 80, 36, 2, 6, [167, 122, 76, 255]);
        rect(&mut image, 77, 34, 8, 3, [133, 148, 154, 255]);
        rect(&mut image, 15, 48, 18, 29, shade(wood, -17));
        for row in 0..2 {
            for book in 0..4 {
                rect(
                    &mut image,
                    17 + book * 4,
                    50 + row * 12,
                    3,
                    9,
                    [
                        [104, 137, 119, 255],
                        [161, 106, 80, 255],
                        [180, 155, 99, 255],
                        [105, 117, 151, 255],
                    ][book as usize],
                );
            }
            rect(&mut image, 15, 60 + row * 12, 18, 3, wood);
        }
        // Stool and delivery chest.
        rect(&mut image, 109, 75, 3, 10, wood);
        rect(&mut image, 121, 75, 3, 10, wood);
        rect(&mut image, 107, 72, 19, 5, shade(wood, 15));
        rect(&mut image, 17, 83, 18, 10, shade(wood, -12));
        rect(&mut image, 17, 81, 18, 4, wood);
        rect(&mut image, 24, 81, 3, 12, accent);
        image
    });
    if state != State::Disconnected {
        let flicker = (tick % 3) as u32;
        rect(
            &mut image,
            109,
            37 - flicker,
            8,
            7 + flicker,
            [216, 104, 55, 255],
        );
        rect(&mut image, 112, 39 - flicker, 3, 5 + flicker, accent);
    }
    if inhabited {
        let (x, y, direction) = match state {
            State::Working => (61, 32, 3),
            State::Waiting => (104, 50, 1),
            State::Done => (18, 62, 0),
            State::Blocked => (63, 52, 0),
            State::Disconnected => (116, 65, 0),
            State::Connected => (63, 53, 0),
        };
        if let Some(sheet) = art
            .characters
            .get((appearance / 8) % art.characters.len().max(1))
        {
            let character = appearance % 8;
            let frame = if state == State::Working {
                [0, 1, 2, 1][tick as usize % 4]
            } else {
                1
            };
            let fw = scene.framewidth;
            let fh = scene.frameheight;
            let sprite = imageops::crop_imm(
                sheet,
                (character as u32 % 4 * 3 + frame) * fw,
                (character as u32 / 4 * 4 + direction) * fh,
                fw,
                fh,
            )
            .to_image();
            imageops::overlay(&mut image, &sprite, x, y);
        } else {
            // Original tiny sprite for source builds without the optional paid pack.
            let coat = [
                [91, 129, 121, 255],
                [117, 100, 151, 255],
                [165, 100, 72, 255],
                [101, 128, 157, 255],
            ][appearance % 4];
            rect(
                &mut image,
                x as u32 + 6,
                y as u32 + 8,
                10,
                9,
                [213, 174, 129, 255],
            );
            rect(
                &mut image,
                x as u32 + 5,
                y as u32 + 5,
                12,
                5,
                shade(coat, -25),
            );
            rect(&mut image, x as u32 + 4, y as u32 + 17, 14, 10, coat);
            rect(
                &mut image,
                x as u32 + 6,
                y as u32 + 27,
                4,
                5,
                [53, 48, 57, 255],
            );
            rect(
                &mut image,
                x as u32 + 13,
                y as u32 + 27,
                4,
                5,
                [53, 48, 57, 255],
            );
        }
        if state == State::Working && tick.is_multiple_of(2) {
            for (sx, sy) in [(60, 31), (72, 34), (67, 27)] {
                rect(&mut image, sx, sy, 2, 2, accent);
            }
        }
        if state == State::Blocked {
            rect(&mut image, 81, 49, 2, 6, accent);
            rect(&mut image, 81, 57, 2, 2, accent);
        }
        if state == State::Done {
            rect(&mut image, 19, 78, 14, 3, accent);
        }
    }
    if state == State::Disconnected {
        for pixel in image.pixels_mut() {
            for c in &mut pixel.0[..3] {
                *c = (*c as f32 * 0.48) as u8;
            }
        }
    }
    let mut image = imageops::resize(&image, WIDTH * 4, HEIGHT * 4, imageops::FilterType::Nearest);
    for pixel in image.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(image)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_character_and_state_frames_render_and_work_animates() {
        let plugin = synapsecore::plugins::builtin();
        let art = load(&plugin).unwrap();
        for appearance in 0..32 {
            for state in [
                State::Connected,
                State::Working,
                State::Waiting,
                State::Blocked,
                State::Done,
                State::Disconnected,
            ] {
                let rendered = room(&plugin, &art, appearance, state, 0, true);
                assert_eq!(
                    rendered.as_bytes(0).unwrap().len(),
                    (WIDTH * HEIGHT * 16 * 4) as usize
                );
            }
        }
        let first = room(&plugin, &art, 0, State::Working, 0, true);
        let next = room(&plugin, &art, 0, State::Working, 1, true);
        assert_ne!(first.as_bytes(0), next.as_bytes(0));
        let fallback = Art {
            characters: Vec::new(),
            background: None,
        };
        assert!(
            room(&plugin, &fallback, 0, State::Waiting, 0, true)
                .as_bytes(0)
                .is_some()
        );
    }
}
