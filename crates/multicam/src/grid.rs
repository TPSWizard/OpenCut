//! The multicam grid (row 102): every angle of a group drawn small at
//! once, so the editor can watch them all and cut to one by its number.
//! Both editors draw the grid by placing each angle in a cell of the
//! canvas (these placements) and rendering that instead of the program;
//! the cut list and the angle that is live come from the same functions.

use crate::Cut;

/// The most angles the grid draws (keys 1-9 reach the first nine).
pub const GRID_MAX_ANGLES: usize = 16;
/// The space between cells and round the edge, as a share of the
/// canvas's shorter side.
pub const GRID_GAP: f64 = 0.01;

/// One angle's cell and how its clip is placed to fill it without
/// cropping: `cell` is `[x, y, width, height]` in canvas pixels from the
/// top left; the position (canvas pixels from the centre, y down) and
/// scale are the editors' transform params, the scale relative to the
/// clip fitted to the whole canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridPlacement {
    pub cell: [f64; 4],
    pub position_x: f64,
    pub position_y: f64,
    pub scale: f64,
}

/// Columns and rows for `count` angles: as square as it goes, columns first.
pub fn grid_shape(count: usize) -> (usize, usize) {
    let count = count.clamp(1, GRID_MAX_ANGLES);
    let columns = (count as f64).sqrt().ceil() as usize;
    (columns, count.div_ceil(columns))
}

/// The cells of `count` angles on a `canvas`, in angle order: rows left to
/// right, top to bottom, a short last row centred.
pub fn grid_cells(count: usize, canvas: (f64, f64)) -> Vec<[f64; 4]> {
    let count = count.min(GRID_MAX_ANGLES);
    if count == 0 {
        return Vec::new();
    }
    let (width, height) = (canvas.0.max(1.0), canvas.1.max(1.0));
    let (columns, rows) = grid_shape(count);
    let gap = GRID_GAP * width.min(height);
    let cell_width = ((width - gap * (columns as f64 + 1.0)) / columns as f64).max(1.0);
    let cell_height = ((height - gap * (rows as f64 + 1.0)) / rows as f64).max(1.0);
    (0..count)
        .map(|index| {
            let (row, column) = (index / columns, index % columns);
            let in_row = (count - row * columns).min(columns);
            let offset = (columns - in_row) as f64 * (cell_width + gap) / 2.0;
            let x = gap + column as f64 * (cell_width + gap) + offset;
            let y = gap + row as f64 * (cell_height + gap);
            [x, y, cell_width, cell_height]
        })
        .collect()
}

/// Where clips of `sources` sizes (one per angle, in order) go in the grid.
pub fn grid_placements(canvas: (f64, f64), sources: &[(f64, f64)]) -> Vec<GridPlacement> {
    let (width, height) = (canvas.0.max(1.0), canvas.1.max(1.0));
    grid_cells(sources.len(), (width, height))
        .into_iter()
        .zip(sources)
        .map(|(cell, &(source_width, source_height))| {
            let (source_width, source_height) = (source_width.max(1.0), source_height.max(1.0));
            let canvas_fit = (width / source_width).min(height / source_height);
            let (fit_width, fit_height) = (source_width * canvas_fit, source_height * canvas_fit);
            let [x, y, cell_width, cell_height] = cell;
            GridPlacement {
                cell,
                position_x: x + cell_width / 2.0 - width / 2.0,
                position_y: y + cell_height / 2.0 - height / 2.0,
                scale: (cell_width / fit_width).min(cell_height / fit_height),
            }
        })
        .collect()
}

/// The angle whose cell holds canvas point `(x, y)`, if any.
pub fn grid_cell_at(count: usize, canvas: (f64, f64), x: f64, y: f64) -> Option<usize> {
    grid_cells(count, canvas)
        .iter()
        .position(|&[left, top, cell_width, cell_height]| x >= left && x < left + cell_width && y >= top && y < top + cell_height)
}

/// The angle the program shows at timeline second `time`: the last cut at
/// or before it, else the first angle.
pub fn angle_at(cuts: &[Cut], time: f64) -> usize {
    let mut sorted = cuts.to_vec();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    sorted.iter().filter(|cut| cut.time <= time + 1e-9).last().map_or(0, |cut| cut.angle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/multicam/grid.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn pair(value: &Value) -> (f64, f64) {
        (value[0].as_f64().unwrap(), value[1].as_f64().unwrap())
    }

    #[test]
    fn grids_match_the_reference() {
        let cases = fixture();
        let grids = cases["grids"].as_array().unwrap();
        assert!(grids.len() >= 8);
        for case in grids {
            let canvas = pair(&case["canvas"]);
            let sources: Vec<(f64, f64)> = case["sources"].as_array().unwrap().iter().map(pair).collect();
            assert_eq!(grid_shape(sources.len()), (case["columns"].as_u64().unwrap() as usize, case["rows"].as_u64().unwrap() as usize));
            let placements = grid_placements(canvas, &sources);
            let expected = case["placements"].as_array().unwrap();
            assert_eq!(placements.len(), expected.len(), "{}", case["name"]);
            for (got, want) in placements.iter().zip(expected) {
                let close = |a: f64, b: &Value| (a - b.as_f64().unwrap()).abs() < 1e-9;
                for (index, value) in got.cell.iter().enumerate() {
                    assert!(close(*value, &want["cell"][index]), "{}: cell {:?} vs {}", case["name"], got.cell, want["cell"]);
                }
                assert!(close(got.position_x, &want["positionX"]) && close(got.position_y, &want["positionY"]), "{}", case["name"]);
                assert!(close(got.scale, &want["scale"]), "{}: scale {}", case["name"], got.scale);
            }
            for hit in case["hits"].as_array().unwrap() {
                let want = hit[2].as_i64().unwrap();
                let got = grid_cell_at(sources.len(), canvas, hit[0].as_f64().unwrap(), hit[1].as_f64().unwrap());
                assert_eq!(got.map_or(-1, |index| index as i64), want, "{} at {hit}", case["name"]);
            }
        }
    }

    #[test]
    fn live_angles_match_the_reference() {
        for case in fixture()["live"].as_array().unwrap() {
            let cuts = crate::parse_cuts(case["cuts"].as_str().unwrap());
            for probe in case["probes"].as_array().unwrap() {
                let time = probe[0].as_f64().unwrap();
                assert_eq!(angle_at(&cuts, time), probe[1].as_u64().unwrap() as usize, "{} at {time}", case["cuts"]);
            }
        }
    }
}
