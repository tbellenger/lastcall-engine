use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::model::Polygon;

/// Integer square root (floor).
pub fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Even-odd fill. The bottom and right edges of an edge pair are exclusive,
/// so a rectangle (0,0)-(10,0)-(10,10)-(0,10) covers x=0..9 and y=0..9.
pub fn rasterize(width: i32, height: i32, polygons: &[Polygon]) -> Vec<u8> {
    let mut mask = vec![0u8; (width * height) as usize];
    for poly in polygons {
        let mut rings: Vec<&[(i32, i32)]> = Vec::new();
        let outer: Vec<(i32, i32)> = poly.points.iter().map(|p| (p[0], p[1])).collect();
        rings.push(&outer);
        let holes: Vec<Vec<(i32, i32)>> = poly
            .holes
            .iter()
            .map(|h| h.iter().map(|p| (p[0], p[1])).collect())
            .collect();
        for hole in &holes {
            rings.push(hole);
        }
        fill_even_odd(&mut mask, width, height, &rings);
    }
    mask
}

fn fill_even_odd(mask: &mut [u8], width: i32, height: i32, rings: &[&[(i32, i32)]]) {
    for y in 0..height {
        let mut xs = Vec::new();
        for ring in rings {
            if ring.len() < 3 {
                continue;
            }
            for i in 0..ring.len() {
                let (x0, y0) = ring[i];
                let (x1, y1) = ring[(i + 1) % ring.len()];
                if y0 == y1 {
                    continue;
                }
                let (ymin, ymax) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
                if y < ymin || y >= ymax {
                    continue;
                }
                let denom = y1 - y0;
                let x = x0 + (x1 - x0) * (y - y0) / denom;
                xs.push(x);
            }
        }
        xs.sort_unstable();
        for pair in xs.as_chunks::<2>().0 {
            let mut x0 = pair[0].max(0);
            let x1 = pair[1].min(width);
            while x0 < x1 {
                if x0 < width {
                    mask[(y * width + x0) as usize] = 255;
                }
                x0 += 1;
            }
        }
    }
}

pub fn in_mask(mask: &[u8], width: i32, height: i32, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x >= width || y >= height {
        return false;
    }
    mask[(y * width + x) as usize] != 0
}

pub fn can_step(mask: &[u8], width: i32, height: i32, x: i32, y: i32, nx: i32, ny: i32) -> bool {
    if !in_mask(mask, width, height, nx, ny) {
        return false;
    }
    let dx = nx - x;
    let dy = ny - y;
    if dx != 0 && dy != 0 {
        return in_mask(mask, width, height, x + dx, y) && in_mask(mask, width, height, x, y + dy);
    }
    true
}

const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// Flood the walk mask with the same 8-direction corner rule as pathfinding.
pub fn flood(mask: &[u8], width: i32, height: i32, start: (i32, i32)) -> Vec<bool> {
    let mut seen = vec![false; (width * height) as usize];
    if !in_mask(mask, width, height, start.0, start.1) {
        return seen;
    }
    let mut stack = vec![start];
    seen[(start.1 * width + start.0) as usize] = true;
    while let Some((x, y)) = stack.pop() {
        for (dx, dy) in DIRS {
            let nx = x + dx;
            let ny = y + dy;
            if !can_step(mask, width, height, x, y, nx, ny) {
                continue;
            }
            let idx = (ny * width + nx) as usize;
            if !seen[idx] {
                seen[idx] = true;
                stack.push((nx, ny));
            }
        }
    }
    seen
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Node {
    f: u32,
    seq: u32,
    idx: i32,
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.cmp(&self.f).then(other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 8-direction A* on the walk mask. Costs are 10 (cardinal) and 14 (diagonal).
/// The path includes both endpoints. Returns `None` when no path exists.
pub fn astar(
    mask: &[u8],
    width: i32,
    height: i32,
    start: (i32, i32),
    goal: (i32, i32),
) -> Option<Vec<(i32, i32)>> {
    if !in_mask(mask, width, height, start.0, start.1)
        || !in_mask(mask, width, height, goal.0, goal.1)
    {
        return None;
    }
    if start == goal {
        return Some(vec![start]);
    }
    let n = (width * height) as usize;
    let mut g = vec![u32::MAX; n];
    let mut parent = vec![-1i32; n];
    let mut closed = vec![false; n];
    let mut open = BinaryHeap::new();
    let start_i = start.1 * width + start.0;
    g[start_i as usize] = 0;
    open.push(Node {
        f: heuristic(start, goal),
        seq: 0,
        idx: start_i,
    });
    let mut seq = 1u32;
    while let Some(Node { idx, .. }) = open.pop() {
        if closed[idx as usize] {
            continue;
        }
        closed[idx as usize] = true;
        let x = idx % width;
        let y = idx / width;
        if (x, y) == goal {
            return Some(rebuild(parent, width, idx));
        }
        for (dx, dy) in DIRS {
            let nx = x + dx;
            let ny = y + dy;
            if !can_step(mask, width, height, x, y, nx, ny) {
                continue;
            }
            let nidx = ny * width + nx;
            if closed[nidx as usize] {
                continue;
            }
            let step = if dx != 0 && dy != 0 { 14 } else { 10 };
            let ng = g[idx as usize].saturating_add(step);
            if ng < g[nidx as usize] {
                g[nidx as usize] = ng;
                parent[nidx as usize] = idx;
                open.push(Node {
                    f: ng.saturating_add(heuristic((nx, ny), goal)),
                    seq,
                    idx: nidx,
                });
                seq = seq.wrapping_add(1);
            }
        }
    }
    None
}

fn heuristic(a: (i32, i32), b: (i32, i32)) -> u32 {
    let dx = (a.0 - b.0).unsigned_abs();
    let dy = (a.1 - b.1).unsigned_abs();
    let (min, max) = if dx < dy { (dx, dy) } else { (dy, dx) };
    min * 14 + (max - min) * 10
}

fn rebuild(parent: Vec<i32>, width: i32, mut idx: i32) -> Vec<(i32, i32)> {
    let mut path = Vec::new();
    loop {
        path.push((idx % width, idx / width));
        if parent[idx as usize] < 0 {
            break;
        }
        idx = parent[idx as usize];
    }
    path.reverse();
    path
}

pub fn point_in_rect(x: i32, y: i32, rect: [i32; 4]) -> bool {
    x >= rect[0] && y >= rect[1] && x < rect[0] + rect[2] && y < rect[1] + rect[3]
}

pub fn nearest_walkable(
    mask: &[u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    radius: i32,
) -> Option<(i32, i32)> {
    if in_mask(mask, width, height, x, y) {
        return Some((x, y));
    }
    let mut best: Option<(i64, i32, i32)> = None;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let nx = x + dx;
            let ny = y + dy;
            if !in_mask(mask, width, height, nx, ny) {
                continue;
            }
            let dist = dx as i64 * dx as i64 + dy as i64 * dy as i64;
            let replace = match best {
                None => true,
                Some((bd, by, bx)) => {
                    dist < bd || (dist == bd && (ny < by || (ny == by && nx < bx)))
                }
            };
            if replace {
                best = Some((dist, ny, nx));
            }
        }
    }
    best.map(|(_, y, x)| (x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_known() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(8), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(3 * 3 + 4 * 4), 5);
    }

    #[test]
    fn rect_and_hole() {
        let poly = Polygon {
            points: vec![[0, 0], [10, 0], [10, 10], [0, 10]],
            holes: vec![vec![[3, 3], [7, 3], [7, 7], [3, 7]]],
        };
        let mask = rasterize(10, 10, &[poly]);
        assert_eq!(mask[0], 255);
        assert_eq!(mask[4 * 10 + 4], 0);
        assert_eq!(mask[11], 255);
    }

    #[test]
    fn astar_goes_around_wall() {
        let mut mask = vec![255u8; 10 * 8];
        for y in 0..7 {
            mask[y * 10 + 4] = 0;
        }
        let path = astar(&mask, 10, 8, (1, 3), (7, 3)).expect("path");
        assert_eq!(path.first().copied(), Some((1, 3)));
        assert_eq!(path.last().copied(), Some((7, 3)));
        assert!(path.iter().all(|&(x, y)| mask[(y * 10 + x) as usize] != 0));
    }
}
