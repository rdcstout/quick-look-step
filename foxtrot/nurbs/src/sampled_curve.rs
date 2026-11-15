use nalgebra_glm::{dot, length, length2, DVec3};
use crate::{abstract_curve::AbstractCurve, nd_curve::NDBSplineCurve};

#[derive(Debug)]
pub struct SampledCurve<const N: usize> {
    curve: NDBSplineCurve<N>,
    samples: Vec<(f64, DVec3)>,
    position_tolerance: f64,
}

impl<const N: usize> SampledCurve<N>
    where NDBSplineCurve<N>: AbstractCurve
{
    pub fn new(curve: NDBSplineCurve<N>) -> Self {
        const N: usize = 8;
        let mut samples = Vec::new();
        for i in 0..curve.knots.len() - 1 {
            // Skip multiple knots
            if curve.knots[i] == curve.knots[i + 1] {
                continue;
            }
            // Iterate over a grid within this region
            for u in 0..N {
                let frac = (u as f64) / (N as f64 - 1.0);
                let u = curve.knots[i] * (1.0 - frac) + curve.knots[i + 1] * frac;
                if u < curve.min_u() || u > curve.max_u() {
                    continue;
                }

                let q = curve.point(u);
                samples.push((u, q));
            }
        }

        let origin = samples.first().map_or_else(DVec3::zeros, |(_, point)| *point);
        let span = samples.iter().map(|(_, point)| (point - origin).norm()).fold(0.0, f64::max);
        let position_tolerance = (span * 1e-7).max(1e-12);
        Self { curve, samples, position_tolerance }
    }

    // Section 6.1 (start middle page 232)
    pub fn u_from_point_newtons_method(&self, P: DVec3, u_0: f64) -> f64 {
        let min = self.curve.min_u();
        let max = self.curve.max_u();
        let mut u_i = u_0;
        let mut best = (f64::INFINITY, u_0);
        for _ in 0..64 {
            let derivs = self.curve.derivs::<2>(u_i);
            let C = derivs[0];
            let C_p = derivs[1];
            let C_pp = derivs[2];
            let r = C - P;

            let distance = length(&r);
            if distance < best.0 {
                best = (distance, u_i);
            }
            if distance <= self.position_tolerance {
                return u_i;
            }

            // calculate the next `u`
            // let f(u) = C'(u) dot (C(u) - P)
            // u_{ip1} = u_i - (f(u_i) / f'(u_i)) = u_i - (C'(u_i) dot (C(u_i) - P)) / (C''(u_i) dot (C(u_i) - P) + |C'(u_i)|^2)
            let delta_i = -dot(&C_p, &r) / (dot(&C_pp, &r) + length2(&C_p));
            let candidate = u_i + delta_i;
            if !candidate.is_finite() {
                break;
            }
            let u_ip1 = if self.curve.open {
                candidate.clamp(min, max)
            } else {
                min + (candidate - min).rem_euclid(max - min)
            };
            if u_ip1 == u_i {
                break;
            }

            u_i = u_ip1;
        }
        best.1
    }

    pub fn u_from_point(&self, p: DVec3) -> f64 {
        use ordered_float::OrderedFloat;
        let best_u = self.samples.iter()
            .min_by_key(|(_u, pos)| OrderedFloat((pos - p).norm()))
            .unwrap().0;
        self.u_from_point_newtons_method(p, best_u)
    }

    pub fn as_polyline(&self, u_start: f64, u_end: f64, num_points_per_knot: usize) -> Vec<DVec3> {
        let (u_min, u_max) = if u_start < u_end {
            (u_start, u_end)
        } else {
            (u_end, u_start)
        };

        let mut result = vec![self.curve.point(u_min)];

        // TODO this could be faster if we skip to the right start/end sections

        assert!(num_points_per_knot > 0);
        for i in 0..self.curve.knots.len() - 1 {
            // Skip multiple knots
            if self.curve.knots[i] == self.curve.knots[i + 1] {
                continue;
            }
            // Iterate over a grid within this region
            for u in 0..num_points_per_knot {
                let frac = (u as f64) / (num_points_per_knot as f64);
                let u = self.curve.knots[i] * (1.0 - frac) + self.curve.knots[i + 1] * frac;
                if u > u_min && u < u_max {
                    result.push(self.curve.point(u));
                }
            }
        }
        result.push(self.curve.point(u_max));

        if u_start > u_end {
            result.reverse();
        }
        result
    }

    pub fn edge_polyline(&self, start: f64, end: f64, whole_loop: bool, forward: bool) -> Vec<DVec3> {
        let crosses_seam = if forward { end < start } else { end > start };
        if self.curve.open || (!whole_loop && !crosses_seam) {
            return self.as_polyline(start, end, 8);
        }
        let limits = (self.curve.min_u(), self.curve.max_u());
        let (exit, entry) = if forward { (limits.1, limits.0) } else { limits };
        let mut points = if start == exit {
            vec![self.curve.point(start)]
        } else {
            self.as_polyline(start, exit, 8)
        };
        if entry != end {
            points.extend(self.as_polyline(entry, end, 8).into_iter().skip(1));
        }
        points
    }
}
