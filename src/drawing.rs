use crate::parse_cargo_tree_output::TreeNode;
use std::{cmp, collections::HashSet, rc::Rc};

pub type Point = (f32, f32);
pub type Color = (u8, u8, u8);
fn get_satellites(
    center: Point,
    root_radius: f32,
    in_radius: f32,
    amount: usize,
    phase: f32,
    sky: f32,
) -> (f32, Vec<(Point, f32)>) {
    let diff_angle = sky / (amount as f32);

    (
        if diff_angle > std::f32::consts::PI {
            root_radius * 0.7
        } else {
            f32::min(
                in_radius * (2.0f32.sqrt()) * (1.0 - diff_angle.cos()).sqrt() / 2.0,
                root_radius * 0.7,
            )
        },
        (0u32..(amount as u32))
            // 0 1 2 3 5 6 7 ... to 0 1 1 2 2 3 3 ...
            .map(|idx| ((idx as f32) / 2.0).ceil())
            // Alternate sign
            .zip([1, -1].iter().cycle())
            .map(|(idx, &sign)| idx * (sign as f32))
            // Get final angle for this satellite
            .map(|alternating_idx| phase + alternating_idx * diff_angle)
            // Get final cartesian coordinates of this satellite, also append angle
            .map(|angle: f32| {
                (
                    (
                        center.0 + angle.cos() * in_radius,
                        center.1 + angle.sin() * in_radius,
                    ),
                    angle,
                )
            })
            .collect::<Vec<_>>(),
    )
}

pub struct DrawCrate {
    pub center: Point,
    pub radius: f32,
    pub color: Color,
    pub name: String,
    pub tree: Rc<TreeNode>,
}

pub struct DrawLine {
    pub p1: Point,
    pub p2: Point,
    pub color: Color,
}

pub fn draw_tree(
    center: Point,
    tree: Rc<TreeNode>,
    radius: f32,
    phase: f32,
    depth: usize,
    sky: f32,
    phase_accum: f32,
    color: Color,
    completed: &HashSet<String>,
    active: &HashSet<String>,
    transition: f32,
) -> (Vec<DrawCrate>, Vec<DrawLine>) {
    let mut crate_draws = Vec::<DrawCrate>::new();
    let mut line_draws = Vec::<DrawLine>::new();

    // Draw a red outline if active
    crate_draws.push(DrawCrate {
        center,
        radius,
        color: if active.contains(&tree.name) {
            let active_color = (0x98, 0xfb, 0x98);

            let base_r = cmp::min(color.0, active_color.0);
            let base_g = cmp::min(color.1, active_color.1);
            let base_b = cmp::min(color.2, active_color.2);

            let diff_r = cmp::max(color.0, active_color.0) - base_r;
            let diff_g = cmp::max(color.1, active_color.1) - base_g;
            let diff_b = cmp::max(color.2, active_color.2) - base_b;

            (
                base_r.saturating_add((diff_r as f32 * transition) as u8),
                base_g.saturating_add((diff_g as f32 * transition) as u8),
                base_b.saturating_add((diff_b as f32 * transition) as u8),
            )
        } else if completed.contains(&tree.name) {
            (0x98, 0xfb, 0x98)
        } else {
            color
        },
        name: tree.name.clone(),
        tree: Rc::clone(&tree),
    });

    let child_count = tree.children.len();

    let (new_radius, sats) = get_satellites(
        (center.0, center.1),
        radius,
        radius * 2.0,
        child_count,
        phase + phase_accum,
        sky,
    );

    sats.into_iter()
        .zip(tree.children.iter())
        .for_each(|((point, point_phase), child)| {
            let child_center = if child.children.len() < 5 {
                point
            } else {
                (
                    point.0 + new_radius * point_phase.cos() * 1.5,
                    point.1 + new_radius * point_phase.sin() * 1.5,
                )
            };

            let child_sky = {
                if Rc::clone(&child).children.len() < 5 {
                    std::f32::consts::PI / 2.0
                } else {
                    std::f32::consts::PI * 1.5
                }
            };

            let (child_crate_draws, child_line_draws) = draw_tree(
                child_center,
                Rc::clone(&child),
                new_radius,
                point_phase,
                depth + 1,
                child_sky,
                phase_accum,
                child.color,
                &completed,
                &active,
                transition,
            );

            // Make sure the line starts from the circle and not from the center
            let line_start = (
                center.0 + point_phase.cos() * radius,
                center.1 + point_phase.sin() * radius,
            );

            let line_end = (
                child_center.0 - (point_phase).cos() * new_radius,
                child_center.1 - (point_phase).sin() * new_radius,
            );

            line_draws.push(DrawLine {
                p1: line_start,
                p2: line_end,
                color: (255, 255, 255),
            });

            crate_draws.extend(child_crate_draws);
            line_draws.extend(child_line_draws);
        });

    (crate_draws, line_draws)
}
