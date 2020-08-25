use crate::dependency_tree::{Dependency, DependencyIterator};
use crc::crc32;
use nannou::draw;
use nannou::prelude::*;
use std::collections::HashSet;

pub type Phase = f32;
pub type Transition = f32;
pub type Color = Rgb<u8>;

#[derive(Copy, Clone)]
pub struct Point(pub f32, pub f32);

const COMPLETED_TASK_FILL: (u8, u8, u8) = (0x98, 0xfb, 0x98);
const COMPLETED_TASK_STROKE: (u8, u8, u8) = (0x00, 0x80, 0x00);

enum DrawStage {
    Line {
        p1: Point,
        p2: Point,
    },
    Crate {
        center: Point,
        radius: f32,
        color: Color,
        name: String,
    },
}

struct DrawLevel<'a> {
    center: Point,
    radius: f32,
    iter: DependencyIterator<'a>,
}

struct DrawingPlan<'a> {
    stack: Vec<DrawLevel<'a>>,
    angle: f32,
    phase_accum: Phase,
}

impl<'a> DrawingPlan<'a> {
    fn new(root: Dependency<'a>, phase_accum: Phase) -> DrawingPlan<'a> {
        let root = DrawLevel {
            center: Point(0.0, 0.0),
            radius: 150.0,
            iter: root.into_iter(),
        };

        DrawingPlan {
            stack: vec![root],
            angle: 1.0,
            phase_accum,
        }
    }
}

impl<'a> Iterator for DrawingPlan<'a> {
    type Item = DrawStage;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(mut level) = self.stack.pop() {
            let children_count = level.iter.len();
            let parent_sky = calc_sky(self.stack.len(), children_count);
            let diff_angle = parent_sky / (children_count as f32);

            if let Some(child) = level.iter.next() {
                // Going down, return a line
                let idx = level.iter.index().unwrap();
                let base_angle = if idx == 0 {
                    self.angle
                } else {
                    self.angle - calc_phase_change(idx - 1, diff_angle)
                };
                let child_angle =
                    base_angle + calc_phase_change(idx, diff_angle) + self.phase_accum;
                let child_radius = calc_child_radius(level.radius, diff_angle);
                let child_center = calc_child_center(
                    level.center,
                    level.radius,
                    child_radius,
                    child_angle,
                    child.children_count(),
                );

                let draw_line = DrawStage::Line {
                    p1: Point(
                        level.center.0 + child_angle.cos() * level.radius,
                        level.center.1 + child_angle.sin() * level.radius,
                    ),
                    p2: Point(
                        child_center.0 - child_angle.cos() * child_radius,
                        child_center.1 - child_angle.sin() * child_radius,
                    ),
                };

                self.angle = child_angle;
                self.stack.push(level);
                self.stack.push(DrawLevel {
                    center: child_center,
                    radius: child_radius,
                    iter: child.into_iter(),
                });

                Some(draw_line)
            } else {
                // Going up, return a crate
                let name = level.iter.name().to_string();
                let crc = crc32::checksum_ieee(name.as_bytes());

                let base_angle = if children_count == 0 {
                    self.angle
                } else {
                    self.angle - calc_phase_change(children_count - 1, diff_angle)
                };
                self.angle = base_angle - self.phase_accum;

                Some(DrawStage::Crate {
                    center: level.center,
                    radius: level.radius,
                    color: Color::new(crc as u8, (crc >> 8) as u8, (crc >> 16) as u8),
                    name,
                })
            }
        } else {
            None
        }
    }
}

fn calc_phase_change(idx: usize, diff_angle: f32) -> f32 {
    let sign = if idx % 2 == 1 { -1f32 } else { 1f32 };
    ((idx as f32) / 2.0).ceil() * diff_angle * sign
}

fn calc_sky(depth: usize, children_count: usize) -> f32 {
    if depth == 0 {
        PI * 2.0
    } else if children_count < 5 {
        PI / 2.0
    } else {
        PI * 1.5
    }
}

fn calc_child_radius(radius: f32, diff_angle: f32) -> f32 {
    if diff_angle > std::f32::consts::PI {
        radius * 0.7
    } else {
        f32::min(
            radius * 2.0 * (2.0f32.sqrt()) * (1.0 - diff_angle.cos()).sqrt() / 2.0,
            radius * 0.7,
        )
    }
}

fn calc_child_center(
    parent_center: Point,
    parent_radius: f32,
    radius: f32,
    angle: f32,
    children: usize,
) -> Point {
    let point = Point(
        parent_center.0 + angle.cos() * parent_radius * 2.0,
        parent_center.1 + angle.sin() * parent_radius * 2.0,
    );

    if children < 5 {
        point
    } else {
        Point(
            point.0 + radius * angle.cos() * 1.5,
            point.1 + radius * angle.sin() * 1.5,
        )
    }
}

pub(crate) fn find_dep_by_pos<'a>(
    active: Dependency<'a>,
    Point(x1, y1): Point,
    phase: Phase,
) -> Option<String> {
    for draw_crate in DrawingPlan::new(active, phase) {
        if let DrawStage::Crate {
            center,
            radius,
            name,
            ..
        } = draw_crate
        {
            let Point(x2, y2) = center;

            if (x2 - x1).powf(2.0) + (y2 - y1).powf(2.0) < radius.powf(2.0) {
                return Some(name.to_string());
            }
        }
    }

    None
}

pub(crate) fn draw_deps<'a>(
    draw: &draw::Draw,
    phase: Phase,
    transition: Transition,
    root: Dependency<'a>,
    completed_tasks: &HashSet<String>,
    active_tasks: &HashSet<String>,
) {
    for stage in DrawingPlan::new(root, phase) {
        match stage {
            DrawStage::Line { p1, p2 } => {
                draw.line()
                    .start(pt2(p1.0, p1.1))
                    .end(pt2(p2.0, p2.1))
                    .weight(2.0)
                    .color(srgba(255u8, 255, 255, 127));
            }
            DrawStage::Crate {
                center,
                radius,
                color,
                name,
            } => {
                let (fill, stroke) = if completed_tasks.contains(&name) {
                    (COMPLETED_TASK_FILL.into(), COMPLETED_TASK_STROKE.into())
                } else if active_tasks.contains(&name) {
                    let color = color_transition(color, COMPLETED_TASK_FILL.into(), transition);
                    (color, COMPLETED_TASK_FILL.into())
                } else {
                    (color, color)
                };

                draw.ellipse()
                    .color(srgba(fill.red, fill.green, fill.blue, 127))
                    .stroke(srgba(stroke.red, stroke.green, stroke.blue, 127))
                    .stroke_weight(if radius < 4.0 || fill == stroke {
                        0.0
                    } else {
                        3.0
                    })
                    .x_y(center.0, center.1)
                    .w_h(radius * 2.0, radius * 2.0);

                if radius > 5.0 {
                    draw.text(&name)
                        .color(WHITE)
                        .x_y(center.0, center.1)
                        .w_h(200.0, 200.0);
                }
            }
        }
    }
}

fn color_transition(a: Color, b: Color, transition: Transition) -> Color {
    let (r1, g1, b1) = a.into_components();
    let (r2, g2, b2) = b.into_components();

    let base_r = ((1.0 - transition) * (r1 as f32)) as u8;
    let base_g = ((1.0 - transition) * (g1 as f32)) as u8;
    let base_b = ((1.0 - transition) * (b1 as f32)) as u8;

    Rgb::new(
        base_r.saturating_add((transition * (r2 as f32)) as u8),
        base_g.saturating_add((transition * (g2 as f32)) as u8),
        base_b.saturating_add((transition * (b2 as f32)) as u8),
    )
}
