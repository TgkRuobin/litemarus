//! Bounding box helpers — behavior matches `litemapy.boxes` (including the z-bounds quirk).

pub type Coordinate = (i32, i32, i32);
pub type BoxCorners = (Coordinate, Coordinate);

pub fn block_is_in_box(block: Coordinate, bbox: BoxCorners) -> bool {
    let (x, y, z) = block;
    let mut xs = [bbox.0 .0, bbox.1 .0];
    xs.sort();
    let mut ys = [bbox.0 .1, bbox.1 .1];
    ys.sort();
    // Python litemapy uses box[0][1] and box[1][1] for zs (replicates original).
    let mut zs = [bbox.0 .1, bbox.1 .1];
    zs.sort();
    let (x_min, x_max) = (xs[0], xs[1]);
    let (y_min, y_max) = (ys[0], ys[1]);
    let (z_min, z_max) = (zs[0], zs[1]);
    x_min <= x && x <= x_max && y_min <= y && y <= y_max && z_min <= z && z <= z_max
}

pub fn box_is_in_box(box1: BoxCorners, box2: BoxCorners) -> bool {
    block_is_in_box(box1.0, box2) && block_is_in_box(box1.1, box2)
}
