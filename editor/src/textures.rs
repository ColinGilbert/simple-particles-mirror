pub const GRID_COLUMNS: u32 = 4;
pub const GRID_ROWS: u32 = 4;
const CELL: u32 = 64;

pub fn grid_sheet() -> (Vec<u8>, u32, u32) {
    let width = GRID_COLUMNS * CELL;
    let height = GRID_ROWS * CELL;
    let frame_count = GRID_COLUMNS * GRID_ROWS;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for row in 0..GRID_ROWS {
        for column in 0..GRID_COLUMNS {
            let frame = row * GRID_COLUMNS + column;
            let progress = frame as f32 / (frame_count - 1) as f32;
            let radius = 0.75f32.mul_add(progress, 0.2) * CELL as f32 / 2.0;
            let [red, green, blue] = hue(progress);
            for y in 0..CELL {
                for x in 0..CELL {
                    let center = CELL as f32 / 2.0;
                    let distance = (x as f32 + 0.5 - center).hypot(y as f32 + 0.5 - center);
                    let alpha = (1.0 - (distance - radius)).clamp(0.0, 1.0);
                    let pixel_x = column * CELL + x;
                    let pixel_y = row * CELL + y;
                    let offset = ((pixel_y * width + pixel_x) * 4) as usize;
                    pixels[offset] = (red * 255.0) as u8;
                    pixels[offset + 1] = (green * 255.0) as u8;
                    pixels[offset + 2] = (blue * 255.0) as u8;
                    pixels[offset + 3] = (alpha * 255.0) as u8;
                }
            }
        }
    }
    (pixels, width, height)
}

fn hue(progress: f32) -> [f32; 3] {
    let angle = progress * 6.0;
    let x = 1.0 - (angle % 2.0 - 1.0).abs();
    match angle as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}
