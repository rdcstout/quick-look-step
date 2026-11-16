use nalgebra_glm::{dot, length, length2, DMat2x2, DVec2, DVec3};
use crate::{abstract_surface::AbstractSurface, nd_surface::NDBSplineSurface};
use log::error;

#[derive(Debug, Clone)]
pub struct SampledSurface<const N: usize> {
    pub surf: NDBSplineSurface<N>,
    samples: Vec<(DVec2, DVec3)>,
    position_tolerance: f64,
}

impl<const N: usize> SampledSurface<N>
    where NDBSplineSurface<N>: AbstractSurface
{
    pub fn new(surf: NDBSplineSurface<N>) -> Self {
        const STEPS: usize = 32;
        let mut samples = Vec::with_capacity((STEPS + 1) * (STEPS + 1));
        for i in 0..=STEPS {
            let u = surf.min_u() + (surf.max_u() - surf.min_u()) * i as f64 / STEPS as f64;
            let u_span = surf.u_knots.find_span(u);
            let u_basis = surf.u_knots.basis_funs_for_span(u_span, u);
            for j in 0..=STEPS {
                let v = surf.min_v() + (surf.max_v() - surf.min_v()) * j as f64 / STEPS as f64;
                let v_span = surf.v_knots.find_span(v);
                let v_basis = surf.v_knots.basis_funs_for_span(v_span, v);
                let point = surf.point_from_basis(u_span, &u_basis, v_span, &v_basis);
                samples.push((DVec2::new(u, v), point));
            }
        }
        let origin = samples[0].1;
        let span = samples.iter().map(|(_, point)| (point - origin).norm()).fold(0.0, f64::max);
        let position_tolerance = (span * 1e-7).max(1e-12);
        Self { surf, samples, position_tolerance }
    }

    // Iteratively project a world-space point onto the spline, staying in its domain.
    fn uv_from_point_newtons_method(
        &self,
        point: DVec3,
        initial: DVec2,
        allowed_residual: f64,
    ) -> Option<DVec2> {
        let mut uv = initial;
        let mut best = (f64::INFINITY, initial);
        for _ in 0..64 {
            let derivs = self.surf.derivs::<2>(uv);
            let offset = derivs[0][0] - point;
            let distance = length(&offset);
            if distance < best.0 {
                best = (distance, uv);
            }
            if distance <= self.position_tolerance {
                return Some(uv);
            }

            let u_tangent = derivs[1][0];
            let v_tangent = derivs[0][1];
            let gradient = DVec2::new(dot(&offset, &u_tangent), dot(&offset, &v_tangent));
            let hessian = symmetric2x2(
                length2(&u_tangent) + dot(&offset, &derivs[2][0]),
                dot(&u_tangent, &v_tangent) + dot(&offset, &derivs[1][1]),
                length2(&v_tangent) + dot(&offset, &derivs[0][2]),
            );
            let Some(inverse) = hessian.try_inverse() else { break };
            let candidate = uv - inverse * gradient;
            if !candidate.iter().all(|value| value.is_finite()) {
                break;
            }
            let next = DVec2::new(
                restrict_parameter(candidate.x, self.surf.min_u(), self.surf.max_u(), self.surf.u_open),
                restrict_parameter(candidate.y, self.surf.min_v(), self.surf.max_v(), self.surf.v_open),
            );
            if uv == next {
                break;
            }
            uv = next;
        }
        if best.0 <= allowed_residual {
            Some(best.1)
        } else {
            error!("Could not find UV coordinates");
            None
        }
    }

    pub fn uv_from_point(&self, p: DVec3, use_periodic_trim: bool) -> Option<DVec2> {
        assert!(!self.samples.is_empty());
        use ordered_float::OrderedFloat;
        let best_uv = self.samples.iter()
            .min_by_key(|(_uv, pos)| OrderedFloat((pos - p).norm()))
            .unwrap().0;
        // STEP trim curves are stored separately from the spline surface.
        // Allow small differences on the periodic trim path, retaining the
        // original 3D curve samples so the boundary still meets adjacent faces.
        let residual_factor = if use_periodic_trim { 3000.0 } else { 1000.0 };
        let allowed_residual = self.position_tolerance * residual_factor;
        self.uv_from_point_newtons_method(p, best_uv, allowed_residual)
    }
}

fn restrict_parameter(value: f64, min: f64, max: f64, open: bool) -> f64 {
    if open || (min..=max).contains(&value) {
        value.clamp(min, max)
    } else {
        min + (value - min).rem_euclid(max - min)
    }
}

/// Builds the symmetric matrix [[a, b], [b, d]]
fn symmetric2x2(a: f64, b: f64, d: f64) -> DMat2x2 {
    // In column major order; because it's symmetric, it doesn't matter
    let mut mat = DMat2x2::identity();
    mat.set_column(0, &DVec2::new(a, b));
    mat.set_column(1, &DVec2::new(b, d));
    mat
}
