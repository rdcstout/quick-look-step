use std::collections::{HashMap, HashSet};
use std::convert::TryInto;

use nalgebra_glm as glm;
use glm::{DVec2, DVec3, DVec4, DMat4, U32Vec3};
use log::{info, warn, error};

#[cfg(feature = "rayon")]
use rayon::prelude::*;

use step::{
    ap214, ap214::*, step_file::{FromEntity, StepFile}, id::Id, ap214::Entity,
};
use crate::{
    Error,
    curve::Curve,
    mesh, mesh::{Mesh, Triangle},
    stats::Stats,
    surface::Surface
};
use nurbs::{BSplineSurface, SampledCurve, SampledSurface, NURBSSurface, KnotVector};

const SAVE_DEBUG_SVGS: bool = false;
const SAVE_PANIC_SVGS: bool = false;
const MAX_REPRESENTATION_VISITS: usize = 250_000;
const MAX_CURVE_REFERENCE_DEPTH: usize = 256;
const MAX_FACE_POINTS: usize = 40_000;
const MAX_MESH_VERTICES: usize = 4_500_000;
const MAX_MESH_TRIANGLES: usize = 5_000_000;

/// `TransformStack` is a mapping of representations to transformed children.
type TransformStack<'a> =
    HashMap<Representation<'a>, Vec<(Representation<'a>, DMat4)>>;

enum RepresentationVisit<'a> {
    Enter(Representation<'a>, DMat4),
    Leave(Representation<'a>),
}

fn build_transform_stack<'a>(s: &'a StepFile, flip: bool) -> TransformStack<'a> {
    // Store a map of parent -> (child, transform)
    let mut transform_stack: HashMap<_, Vec<_>> = HashMap::new();
    for r in s.0.values()
        .filter_map(|e|
            RepresentationRelationshipWithTransformation_::try_from_entity(e))
    {
        let (a, b) = if flip {
            (r.rep_2, r.rep_1)
        } else {
            (r.rep_1, r.rep_2)
        };
        let mut mat = item_defined_transformation(s, r.transformation_operator.cast());
        if flip {
            mat = mat.try_inverse().expect("Could not invert transform matrix");
        }

        transform_stack.entry(b)
            .or_default()
            .push((a, mat));
    }
    transform_stack
}

fn transform_stack_roots<'a>(transform_stack: &TransformStack<'a>) -> Vec<Representation<'a>> {
    let children: HashSet<_> = transform_stack
        .values()
        .flat_map(|v| v.iter())
        .map(|v| v.0)
        .collect();
    transform_stack
        .keys()
        .filter(|k| !children.contains(k))
        .copied()
        .collect()
}

pub fn triangulate(s: &StepFile) -> Result<(Mesh, Stats), Error> {
    let styled_items: Vec<_> = s.0.values()
        .filter_map(|e| MechanicalDesignGeometricPresentationRepresentation_::try_from_entity(e))
        .flat_map(|m| m.items.iter())
        .filter_map(|item| s.entity(item.cast::<StyledItem_>()))
        .collect();
    let brep_colors: HashMap<_, DVec3> = styled_items.iter()
        .filter_map(|styled|
            if styled.styles.len() != 1 {
                None
            } else {
                presentation_style_color(s, styled.styles[0])
                    .map(|c| (styled.item, c))
            })
        .collect();

    // Store a map of parent -> (child, transform)
    let mut transform_stack = build_transform_stack(s, false);
    let mut roots = transform_stack_roots(&transform_stack);
    // The transformation graph isn't directional (because STEP is a Good File
    // Format), so if it's got more than one root, assume it's backwards.  We
    // are assuming that directions in the graph are consistent within the file,
    // until we find a counterexample.
    if roots.len() > 1 {
        info!("Flipping transform stack");
        transform_stack = build_transform_stack(s, true);
        roots = transform_stack_roots(&transform_stack);
    }
    let mut todo: Vec<_> = roots.into_iter()
        .map(|v| RepresentationVisit::Enter(v, DMat4::identity()))
        .collect();
    if todo.len() > 1 {
        warn!("Transformation stack has more than one root!");
    }

    // Store a map of ShapeRepresentationRelationships, which some models
    // use to map from axes to specific instances
    let mut shape_rep_relationship: HashMap<Id<_>, Vec<Id<_>>> = HashMap::new();
    for (r1, r2) in s.0.values()
        .filter_map(|e| ShapeRepresentationRelationship_::try_from_entity(e))
        .map(|e| (e.rep_1, e.rep_2))
    {
        shape_rep_relationship.entry(r1).or_default().push(r2);
    }

    let mut to_mesh: HashMap<Id<_>, Vec<_>> = HashMap::new();
    let mut active_path = HashSet::new();
    let mut visits = 0;
    let mut instances = 0;
    while let Some(visit) = todo.pop() {
        let (id, mat) = match visit {
            RepresentationVisit::Enter(id, mat) => (id, mat),
            RepresentationVisit::Leave(id) => {
                active_path.remove(&id);
                continue;
            }
        };
        visits += 1;
        if visits > MAX_REPRESENTATION_VISITS {
            return Err(Error::RepresentationLimitExceeded);
        }
        if !active_path.insert(id) {
            return Err(Error::CyclicRepresentation);
        }
        let shape_children = shape_rep_relationship.get(&id).map(Vec::as_slice).unwrap_or_default();
        let transform_children = transform_stack.get(&id);
        let pending = todo.len() + shape_children.len() + transform_children.map_or(0, Vec::len);
        if pending > MAX_REPRESENTATION_VISITS {
            return Err(Error::RepresentationLimitExceeded);
        }
        todo.push(RepresentationVisit::Leave(id));
        for child in shape_children {
            todo.push(RepresentationVisit::Enter(*child, mat));
        }
        if let Some(children) = transform_children {
            for (child, next_mat) in children {
                todo.push(RepresentationVisit::Enter(*child, mat * next_mat));
            }
        } else {
            // Bind this transform to the RepresentationItem, which is
            // either a ManifoldSolidBrep or a ShellBasedSurfaceModel
            let items = match &s[id] {
                Entity::AdvancedBrepShapeRepresentation(b) => &b.items,
                Entity::ShapeRepresentation(b) => &b.items,
                Entity::ManifoldSurfaceShapeRepresentation(b) => &b.items,
                Entity::GeometricallyBoundedSurfaceShapeRepresentation(_) => {
                    warn!("Skipping auxiliary bounded surface representation #{}", id.0);
                    continue;
                }
                e => panic!("Could not get shape from {:?}", e),
            };

            for m in items.iter() {
                match &s[*m] {
                    Entity::ManifoldSolidBrep(_)
                    | Entity::BrepWithVoids(_)
                    | Entity::ShellBasedSurfaceModel(_) => {
                        instances += 1;
                        if instances > MAX_REPRESENTATION_VISITS {
                            return Err(Error::RepresentationLimitExceeded);
                        }
                        to_mesh.entry(*m).or_default().push(mat);
                    }
                    Entity::Axis2Placement3d(_) => (),
                    e => warn!("Skipping {:?}", e),
                }
            }
        }
    }
    // If there are items in breps that aren't attached to a transformation
    // chain, then draw them individually (with an identity matrix)
    if to_mesh.is_empty() {
        s.0.iter()
            .filter(|(_i, e)|
                match e {
                    Entity::ManifoldSolidBrep(_)
                    | Entity::BrepWithVoids(_)
                    | Entity::ShellBasedSurfaceModel(_) => true,
                    _ => false,
                }
            )
            .map(|(&i, _e)| Id::new(i))
            .for_each(|i| to_mesh.entry(i).or_default().push(DMat4::identity()));
    }

    let append_solid = |(mut mesh, mut stats): (Mesh, Stats),
                        (id, mats): (&RepresentationItem, &Vec<DMat4>)|
        -> Result<(Mesh, Stats), Error>
    {
        let v_start = mesh.verts.len();
        let t_start = mesh.triangles.len();
        match &s[*id] {
            Entity::ManifoldSolidBrep(b) =>
                closed_shell(s, b.outer, &mut mesh, &mut stats)?,
            Entity::ShellBasedSurfaceModel(b) =>
                for v in &b.sbsm_boundary {
                    shell(s, *v, &mut mesh, &mut stats)?;
                },
            Entity::BrepWithVoids(b) =>
                // TODO: handle voids
                closed_shell(s, b.outer, &mut mesh, &mut stats)?,
            _ => {
                warn!("Skipping {:?} (not a known solid)", s[*id]);
                return Ok((mesh, stats));
            },
        };

        // Pick out a color from the color map and apply it to each
        // newly-created vertex
        let color = brep_colors.get(id)
            .map(|c| *c)
            .unwrap_or(DVec3::new(0.5, 0.5, 0.5));

        // Build copies of the mesh by copying and applying transforms
        let v_end = mesh.verts.len();
        let t_end = mesh.triangles.len();
        let copies = mats.len() - 1;
        enforce_mesh_limit(
            v_end.saturating_add((v_end - v_start).saturating_mul(copies)),
            t_end.saturating_add((t_end - t_start).saturating_mul(copies)),
        )?;
        for mat in &mats[1..] {
            for v in v_start..v_end {
                let p = mesh.verts[v].pos;
                let p_h = DVec4::new(p.x, p.y, p.z, 1.0);
                let pos = (mat * p_h).xyz();

                let n = mesh.verts[v].norm;
                let norm = (mat * glm::vec3_to_vec4(&n)).xyz();

                mesh.verts.push(mesh::Vertex { pos, norm, color });
            }
            let offset = mesh.verts.len() - v_end;
            for t in t_start..t_end {
                let mut tri = mesh.triangles[t];
                tri.verts.add_scalar_mut(offset as u32);
                mesh.triangles.push(tri);
            }
        }

        // Now that we've built all of the other copies of the mesh,
        // re-use the original mesh and apply the first transform
        let mat = mats[0];
        for v in v_start..v_end {
            let p = mesh.verts[v].pos;
            let p_h = DVec4::new(p.x, p.y, p.z, 1.0);
            mesh.verts[v].pos = (mat * p_h).xyz();

            let n = mesh.verts[v].norm;
            mesh.verts[v].norm = (mat * glm::vec3_to_vec4(&n)).xyz();

            mesh.verts[v].color = color;
        }
        Ok((mesh, stats))
    };

    #[cfg(not(feature = "rayon"))]
    let (mesh, stats) = to_mesh.iter().try_fold((Mesh::default(), Stats::default()), append_solid)?;

    #[cfg(feature = "rayon")]
    let (mesh, stats) = to_mesh.par_iter()
        .try_fold(|| (Mesh::default(), Stats::default()), append_solid)
        .try_reduce(
            || (Mesh::default(), Stats::default()),
            |a, b| {
                enforce_mesh_limit(
                    a.0.verts.len().saturating_add(b.0.verts.len()),
                    a.0.triangles.len().saturating_add(b.0.triangles.len()),
                )?;
                Ok((Mesh::combine(a.0, b.0), Stats::combine(a.1, b.1)))
            },
        )?;

    info!("num_shells: {}", stats.num_shells);
    info!("num_faces: {}", stats.num_faces);
    info!("num_errors: {}", stats.num_errors);
    info!("num_panics: {}", stats.num_panics);
    Ok((mesh, stats))
}

fn item_defined_transformation(s: &StepFile, t: Id<ItemDefinedTransformation_>) -> DMat4 {
    let i = s.entity(t).expect("Could not get ItemDefinedTransform");

    let (location, axis, ref_direction) = axis2_placement_3d(s,
        i.transform_item_1.cast());
    let t1 = Surface::make_affine_transform(axis,
        ref_direction,
        axis.cross(&ref_direction),
        location);

    let (location, axis, ref_direction) = axis2_placement_3d(s,
        i.transform_item_2.cast());
    let t2 = Surface::make_affine_transform(axis,
        ref_direction,
        axis.cross(&ref_direction),
        location);

    t2 * t1.try_inverse().expect("Could not invert transform matrix")
}

fn presentation_style_color(s: &StepFile, p: PresentationStyleAssignment)
    -> Option<DVec3>
{
    // AAAAAHHHHH
    s.entity(p)
        .and_then(|p: &PresentationStyleAssignment_| {
                let mut surf = p.styles.iter().filter_map(|y| {
                    // This is an ambiguous parse, so we hard-code the first
                    // Entity item in the enum
                    use PresentationStyleSelect::PreDefinedPresentationStyle;
                    if let PreDefinedPresentationStyle(u) = y {
                        s.entity(u.cast::<SurfaceStyleUsage_>())
                    } else {
                        None
                    }});
                let out = surf.next();
                out
            })
        .and_then(|surf: &SurfaceStyleUsage_|
            s.entity(surf.style.cast::<SurfaceSideStyle_>()))
        .and_then(|surf: &SurfaceSideStyle_| if surf.styles.len() != 1 {
                None
            } else {
                s.entity(surf.styles[0].cast::<SurfaceStyleFillArea_>())
            })
        .map(|surf: &SurfaceStyleFillArea_|
            s.entity(surf.fill_area).expect("Could not get fill_area"))
        .and_then(|fill: &FillAreaStyle_| if fill.fill_styles.len() != 1 {
                None
            } else {
                s.entity(fill.fill_styles[0].cast::<FillAreaStyleColour_>())
            })
        .and_then(|f: &FillAreaStyleColour_|
            s.entity(f.fill_colour.cast::<ColourRgb_>()))
        .map(|c| DVec3::new(c.red, c.green, c.blue))
}

fn cartesian_point(s: &StepFile, a: Id<CartesianPoint_>) -> DVec3 {
    let p = s.entity(a).expect("Could not get cartesian point");
    DVec3::new(p.coordinates[0].0, p.coordinates[1].0, p.coordinates[2].0)
}

fn direction(s: &StepFile, a: Direction) -> DVec3 {
    let p = s.entity(a).expect("Could not get cartesian point");
    DVec3::new(p.direction_ratios[0],
               p.direction_ratios[1],
               p.direction_ratios[2])
}

fn axis2_placement_3d(s: &StepFile, t: Id<Axis2Placement3d_>) -> (DVec3, DVec3, DVec3) {
    let a = s.entity(t).expect("Could not get Axis2Placement3d");
    let location = cartesian_point(s, a.location);
    // TODO: this doesn't necessarily match the behavior of `build_axes`
    let axis = direction(s, a.axis.expect("Missing axis"));
    let ref_direction = match a.ref_direction {
        None => DVec3::new(1.0, 0.0, 0.0),
        Some(r) => direction(s, r),
    };
    (location, axis, ref_direction)
}

fn enforce_mesh_limit(vertices: usize, triangles: usize) -> Result<(), Error> {
    if vertices > MAX_MESH_VERTICES || triangles > MAX_MESH_TRIANGLES {
        return Err(Error::MeshLimitExceeded);
    }
    Ok(())
}

fn shell(s: &StepFile, c: Shell, mesh: &mut Mesh, stats: &mut Stats) -> Result<(), Error> {
    match &s[c] {
        Entity::ClosedShell(_) => closed_shell(s, c.cast(), mesh, stats),
        Entity::OpenShell(_) => {
            let cs = s.entity(c.cast::<OpenShell_>()).expect("Could not get OpenShell");
            triangulate_shell_faces(s, &cs.cfs_faces, mesh, stats)
        }
        h => {
            warn!("Skipping {:?} (unknown Shell type)", h);
            Ok(())
        }
    }
}

fn closed_shell(s: &StepFile, c: ClosedShell, mesh: &mut Mesh, stats: &mut Stats) -> Result<(), Error> {
    let cs = s.entity(c).expect("Could not get ClosedShell");
    triangulate_shell_faces(s, &cs.cfs_faces, mesh, stats)
}

fn triangulate_shell_faces(
    s: &StepFile, faces: &[Face], mesh: &mut Mesh, stats: &mut Stats,
) -> Result<(), Error> {
    for face in faces {
        let start = mesh.verts.len();
        let first_triangle = mesh.triangles.len();
        match advanced_face(s, face.cast(), mesh, stats) {
            Ok(()) if mesh.triangles.len() == first_triangle => mesh.verts.truncate(start),
            Ok(()) => {}
            Err(Error::MeshLimitExceeded) => return Err(Error::MeshLimitExceeded),
            Err(err) => {
                mesh.verts.truncate(start);
                mesh.triangles.truncate(first_triangle);
                stats.num_errors += 1;
                error!("Failed to triangulate face #{}: {}", face.0, err);
            }
        }
        enforce_mesh_limit(mesh.verts.len(), mesh.triangles.len())?;
    }
    stats.num_shells += 1;
    Ok(())
}

fn advanced_face(s: &StepFile, f: AdvancedFace, mesh: &mut Mesh,
                 stats: &mut Stats) -> Result<(), Error>
{
    let face = s.entity(f).expect("Could not get AdvancedFace");
    stats.num_faces += 1;

    // Grab the surface, returning early if it's unimplemented
    let mut surf = get_surface(s, face.face_geometry)?;

    // This is the starting point at which we insert new vertices
    let offset = mesh.verts.len();

    // For each contour, project from 3D down to the surface, then
    // start collecting them as constrained edges for triangulation
    let mut edges = Vec::new();
    let v_start = mesh.verts.len();
    let mut num_pts = 0usize;
    let mut contours = Vec::new();
    let mut contour_points = 0usize;
    for b in &face.bounds {
        let contour = face_bound(s, *b)?;
        contour_points = contour_points.saturating_add(contour.len());
        if contour_points > MAX_FACE_POINTS {
            return Err(Error::MeshLimitExceeded);
        }
        contours.push(contour);
    }
    enforce_mesh_limit(mesh.verts.len() + contour_points, mesh.triangles.len())?;

    if triangulate_periodic_band(&surf, &contours, face.same_sense, mesh)? {
        return Ok(());
    }

    for bound_contours in contours {
        match bound_contours.len() {
            // We should always have non-zero items in the contour
            0 => panic!("Got empty contours for {:?}", face),

            // Special case for a single-vertex point, which shows up in
            // cones: we push it as a Steiner point, but without any
            // associated contours.
            1 => {
                num_pts += 1;
                mesh.verts.push(mesh::Vertex {
                    pos: bound_contours[0],
                    norm: DVec3::zeros(),
                    color: DVec3::new(0.0, 0.0, 0.0),
                });
            },

            // Default for lists of contour points
            _ => {
                // Record the initial point to close the loop
                let start = num_pts;
                for pt in bound_contours {
                    // The contour marches forward!
                    edges.push((num_pts, num_pts + 1));

                    // Also store this vertex in the 3D triangulation
                    mesh.verts.push(mesh::Vertex {
                        pos: pt,
                        norm: DVec3::zeros(),
                        color: DVec3::new(0.0, 0.0, 0.0),
                    });
                    num_pts += 1;
                }
                // The last point is a duplicate, because it closes the
                // contours, so we skip it here and reattach the contour to
                // the start.
                num_pts -= 1;
                mesh.verts.pop();

                // Close the loop by returning to the starting point
                edges.pop();
                edges.last_mut().unwrap().1 = start;
            }
        }
    }

    // We inject Stiner points based on the surface type to improve curvature,
    // e.g. for spherical sections.  However, we don't want triagulation to
    // _fail_ due to these points, so if that happens, we nuke the point (by
    // assigning it to the first point in the list, which causes it to get
    // deduplicated), then retry.
    let mut pts = surf.lower_verts(&mut mesh.verts[v_start..])?;
    let bonus_points = pts.len();
    surf.add_steiner_points(&mut pts, &mut mesh.verts);
    let result = std::panic::catch_unwind(|| {
        // TODO: this is only needed because we use pts below to save a debug
        // SVG if this panics.  Once we're confident in never panicking, we
        // can remove this.
        let mut pts = pts.clone();
        loop {
            let mut t = match cdt::Triangulation::new_with_edges(&pts, &edges) {
                Err(e) => break Err(e),
                Ok(t) => t,
            };
            match t.run() {
                Ok(()) => break Ok(t),
                // If triangulation failed due to a Steiner point on a fixed
                // edge, then reassign that point to pts[0] (so it will be
                // ignored as a duplicate)
                Err(cdt::Error::PointOnFixedEdge(p)) if p >= bonus_points => {
                    pts[p] = pts[0];
                    continue;
                },
                Err(e) => {
                    if SAVE_DEBUG_SVGS {
                        let filename = format!("err{}.svg", face.face_geometry.0);
                        t.save_debug_svg(&filename)
                            .expect("Could not save debug SVG");
                    }
                    break Err(e)
                },
            }
        }
    });
    match result {
        Ok(Ok(t)) => {
            for (a, b, c) in t.triangles() {
                let a = (a + offset) as u32;
                let b = (b + offset) as u32;
                let c = (c + offset) as u32;
                mesh.triangles.push(Triangle { verts:
                    if face.same_sense {
                        U32Vec3::new(a, b, c)
                    } else {
                        U32Vec3::new(a, c, b)
                    }
                });
            }
        },
        Ok(Err(e)) => {
            error!("Got error while triangulating {}: {:?}",
                   face.face_geometry.0, e);
            stats.num_errors += 1;
        },
        Err(e) => {
            error!("Got panic while triangulating {}: {:?}",
                   face.face_geometry.0, e);
            if SAVE_PANIC_SVGS {
                let filename = format!("panic{}.svg", face.face_geometry.0);
                cdt::save_debug_panic(&pts, &edges, &filename)
                    .expect("Could not save debug SVG");
            }
            stats.num_panics += 1;
        }
    }
    // Flip normals of new vertices, depending on the same_sense flag
    if !face.same_sense {
        for v in &mut mesh.verts[v_start..] {
            v.norm = -v.norm;
        }
    }
    Ok(())
}

fn triangulate_periodic_band(
    surface: &Surface,
    contours: &[Vec<DVec3>],
    same_sense: bool,
    mesh: &mut Mesh,
) -> Result<bool, Error> {
    let [first, second] = contours else { return Ok(false) };
    if ![first, second].iter().all(|ring| (4..=256).contains(&ring.len()) && ring.first() == ring.last()) {
        return Ok(false);
    }
    let (min, max, u_open, v_open) = match surface {
        Surface::BSpline(s) => (DVec2::new(s.surf.min_u(), s.surf.min_v()), DVec2::new(s.surf.max_u(), s.surf.max_v()), s.surf.u_open, s.surf.v_open),
        Surface::NURBS(s) => (DVec2::new(s.surf.min_u(), s.surf.min_v()), DVec2::new(s.surf.max_u(), s.surf.max_v()), s.surf.u_open, s.surf.v_open),
        _ => return Ok(false),
    };
    if u_open == v_open {
        return Ok(false);
    }

    let parameter = |across: f64, around: f64| {
        let t = if u_open { DVec2::new(across, around) } else { DVec2::new(around, across) };
        min + (max - min).component_mul(&t)
    };
    const AROUND: usize = 64;
    const ACROSS: usize = 24;
    let boundary = |across| -> Option<Vec<DVec3>> {
        (0..=AROUND).map(|i| surface.raise(parameter(across, i as f64 / AROUND as f64))).collect()
    };
    let (Some(start), Some(end)) = (boundary(0.0), boundary(1.0)) else { return Ok(false) };
    if !(rings_match(first, &start) && rings_match(second, &end)
        || rings_match(first, &end) && rings_match(second, &start))
    {
        return Ok(false);
    }

    let offset = mesh.verts.len();
    enforce_mesh_limit(offset + (ACROSS + 1) * (AROUND + 1), mesh.triangles.len() + 2 * ACROSS * AROUND)?;
    let mut vertices = Vec::with_capacity((ACROSS + 1) * (AROUND + 1));
    for i in 0..=ACROSS {
        for j in 0..=AROUND {
            let uv = parameter(i as f64 / ACROSS as f64, j as f64 / AROUND as f64);
            let Some(pos) = surface.raise(uv) else { return Ok(false) };
            let normal = surface.normal(pos, uv);
            if !pos.iter().chain(normal.iter()).all(|coordinate| coordinate.is_finite()) {
                return Ok(false);
            }
            let norm = if same_sense { normal } else { -normal };
            vertices.push(mesh::Vertex { pos, norm, color: DVec3::zeros() });
        }
    }
    mesh.verts.extend(vertices);
    for i in 0..ACROSS {
        for j in 0..AROUND {
            let a = (offset + i * (AROUND + 1) + j) as u32;
            let b = a + (AROUND + 1) as u32;
            let (x, y) = if same_sense == u_open { (b, a + 1) } else { (a + 1, b) };
            mesh.triangles.push(Triangle { verts: U32Vec3::new(a, x, y) });
            mesh.triangles.push(Triangle { verts: U32Vec3::new(y, x, b + 1) });
        }
    }
    Ok(true)
}

fn rings_match(a: &[DVec3], b: &[DVec3]) -> bool {
    let scale = a.iter().map(|point| (point - a[0]).norm()).fold(0.0, f64::max);
    if !scale.is_finite() || scale <= 0.0 || (b[0] - b[b.len() - 1]).norm() > scale * 1e-6 {
        return false;
    }
    let tolerance = (scale * 0.002).powi(2);
    let near_ring = |point: &DVec3, ring: &[DVec3]| {
        ring.windows(2).any(|segment| {
            let direction = segment[1] - segment[0];
            let length = direction.norm_squared();
            let nearest = if length > 0.0 {
                segment[0] + direction * ((point - segment[0]).dot(&direction) / length).clamp(0.0, 1.0)
            } else {
                segment[0]
            };
            (point - nearest).norm_squared() <= tolerance
        })
    };
    a.iter().all(|point| near_ring(point, b)) && b.iter().all(|point| near_ring(point, a))
}

fn get_surface(s: &StepFile, surf: ap214::Surface) -> Result<Surface, Error> {
    match &s[surf] {
        Entity::CylindricalSurface(c) => {
            let (location, axis, ref_direction) = axis2_placement_3d(s, c.position);
            Ok(Surface::new_cylinder(axis, ref_direction, location, c.radius.0.0.0))
        },
        Entity::ToroidalSurface(c) => {
            let (location, axis, _ref_direction) = axis2_placement_3d(s, c.position);
            Ok(Surface::new_torus(location, axis, c.major_radius.0.0.0, c.minor_radius.0.0.0))
        },
        Entity::Plane(p) => {
            // We'll ignore axis and ref_direction in favor of building an
            // orthonormal basis later on
            let (location, axis, ref_direction) = axis2_placement_3d(s, p.position);
            Ok(Surface::new_plane(axis, ref_direction, location))
        },
        // We treat cones like planes, since that's a valid mapping into 2D
        Entity::ConicalSurface(c) => {
            let (location, axis, ref_direction) = axis2_placement_3d(s, c.position);
            Ok(Surface::new_cone(axis, ref_direction, location, c.semi_angle.0))
        },
        Entity::SphericalSurface(c) => {
            // We'll ignore axis and ref_direction in favor of building an
            // orthonormal basis later on
            let (location, _axis, _ref_direction) = axis2_placement_3d(s, c.position);
            Ok(Surface::new_sphere(location, c.radius.0.0.0))
        },
        Entity::BSplineSurfaceWithKnots(b) =>
        {
            let (u_knot_vec, v_knot_vec) = surface_knots(b)?;

            let control_points_list = control_points_2d(s, &b.control_points_list);

            let surf = BSplineSurface::new(
                b.u_closed.0.unwrap() == false,
                b.v_closed.0.unwrap() == false,
                u_knot_vec,
                v_knot_vec,
                control_points_list,
            );
            Ok(Surface::BSpline(SampledSurface::new(surf)))
        },
        Entity::ComplexEntity(v) if v.len() == 2 => {
            let bspline = if let Entity::BSplineSurfaceWithKnots(b) = &v[0] {
                b
            } else {
                warn!("Could not get BSplineCurveWithKnots from {:?}", v[0]);
                return Err(Error::UnknownCurveType)
            };
            let rational = if let Entity::RationalBSplineSurface(b) = &v[1] {
                b
            } else {
                warn!("Could not get RationalBSplineCurve from {:?}", v[1]);
                return Err(Error::UnknownCurveType)
            };

            let (u_knot_vec, v_knot_vec) = surface_knots(bspline)?;

            let control_points_list = control_points_2d(
                    s, &bspline.control_points_list)
                .into_iter()
                .zip(rational.weights_data.iter())
                .map(|(ctrl, weight)|
                    ctrl.into_iter()
                        .zip(weight.into_iter())
                        .map(|(p, w)| DVec4::new(p.x * w, p.y * w, p.z * w, *w))
                        .collect())
                .collect();

            let surf = NURBSSurface::new(
                bspline.u_closed.0.unwrap() == false,
                bspline.v_closed.0.unwrap() == false,
                u_knot_vec,
                v_knot_vec,
                control_points_list,
            );
            Ok(Surface::NURBS(SampledSurface::new(surf)))

        },
        e => {
            warn!("Could not get surface from {:?}", e);
            Err(Error::UnknownSurfaceType)
        },
    }
}

fn surface_knots(surface: &BSplineSurfaceWithKnots_) -> Result<(KnotVector, KnotVector), Error> {
    let rows = &surface.control_points_list;
    let columns = rows.first().map_or(0, Vec::len);
    if !rows.iter().all(|row| row.len() == columns) {
        return Err(Error::InvalidKnots);
    }
    let u = validated_knots(
        surface.u_degree, surface.u_knots.iter().map(|k| k.0), &surface.u_multiplicities, rows.len(),
    )?;
    let v = validated_knots(
        surface.v_degree, surface.v_knots.iter().map(|k| k.0), &surface.v_multiplicities, columns,
    )?;
    Ok((u, v))
}

fn validated_knots(
    degree: i64,
    knots: impl Iterator<Item = f64>,
    multiplicities: &[i64],
    control_points: usize,
) -> Result<KnotVector, Error> {
    let degree: usize = degree.try_into().map_err(|_| Error::InvalidKnots)?;
    if degree == 0 || degree > 32 || degree >= control_points {
        return Err(Error::InvalidKnots);
    }
    let knots: Vec<_> = knots.collect();
    if knots.len() != multiplicities.len()
        || knots.len() < 2
        || !knots.iter().all(|k| k.is_finite())
        || knots.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(Error::InvalidKnots);
    }

    let counts: Vec<usize> = multiplicities.iter().map(|&count| {
        let count: usize = count.try_into().map_err(|_| Error::InvalidKnots)?;
        if count == 0 || count > degree + 1 {
            return Err(Error::InvalidKnots);
        }
        Ok(count)
    }).collect::<Result<_, _>>()?;
    let expanded_count = counts.iter().try_fold(0usize, |total, &count| total.checked_add(count));
    let expected_count = control_points.checked_add(degree + 1).ok_or(Error::InvalidKnots)?;
    if expanded_count != Some(expected_count) {
        return Err(Error::InvalidKnots);
    }
    Ok(KnotVector::from_multiplicities(degree, &knots, &counts))
}

fn control_points_1d(s: &StepFile, row: &Vec<CartesianPoint>) -> Vec<DVec3> {
    row.iter().map(|p| cartesian_point(s, *p)).collect()
}

fn control_points_2d(s: &StepFile, rows: &Vec<Vec<CartesianPoint>>) -> Vec<Vec<DVec3>> {
    rows.iter()
        .map(|row| control_points_1d(s, row))
        .collect()
}

fn face_bound(s: &StepFile, b: FaceBound) -> Result<Vec<DVec3>, Error> {
    let (bound, orientation) = match &s[b] {
        Entity::FaceBound(b) => (b.bound, b.orientation),
        Entity::FaceOuterBound(b) => (b.bound, b.orientation),
        e => panic!("Could not get bound from {:?} at {:?}", e, b),
    };
    match &s[bound] {
        Entity::EdgeLoop(e) => {
            let mut d = edge_loop(s, &e.edge_list)?;
            if !orientation {
                d.reverse()
            }
            Ok(d)
        },
        Entity::VertexLoop(v) => {
            // This is an "edge loop" with a single vertex, which is
            // used for cones and not really anything else.
            Ok(vec![vertex_point(s, v.loop_vertex)])
        }
        e => panic!("{:?} is not an EdgeLoop", e),
    }
}

fn edge_loop(s: &StepFile, edge_list: &[OrientedEdge])
    -> Result<Vec<DVec3>, Error>
{
    let mut out = Vec::new();
    for (i, e) in edge_list.iter().enumerate() {
        // Remove the last item from the list, since it's the beginning
        // of the following list (hopefully)
        if i > 0 {
            out.pop();
        }
        let edge = s.entity(*e).expect("Could not get OrientedEdge");
        let o = edge_curve(s, edge.edge_element.cast(), edge.orientation)?;
        if out.len().saturating_add(o.len()) > MAX_FACE_POINTS {
            return Err(Error::MeshLimitExceeded);
        }
        out.extend(o.into_iter());
    }
    Ok(out)
}

fn edge_curve(s: &StepFile, e: EdgeCurve, orientation: bool) -> Result<Vec<DVec3>, Error> {
    let edge_curve = s.entity(e).expect("Could not get EdgeCurve");
    let curve = curve(s, edge_curve, edge_curve.edge_geometry, orientation)?;

    let (start, end) = if orientation {
        (edge_curve.edge_start, edge_curve.edge_end)
    } else {
        (edge_curve.edge_end, edge_curve.edge_start)
    };
    let u = vertex_point(s, start);
    let v = vertex_point(s, end);
    Ok(curve.build(u, v, start == end, edge_curve.same_sense ^ !orientation))
}

fn curve<'a>(s: &StepFile<'a>, edge_curve: &ap214::EdgeCurve_<'a>,
         mut curve_id: ap214::Curve<'a>, orientation: bool) -> Result<Curve, Error>
{
    let mut aliases = 0;
    let geometry = loop {
        let next = match &s[curve_id] {
            Entity::SurfaceCurve(v) => v.curve_3d,
            Entity::SeamCurve(v) => v.curve_3d,
            geometry => break geometry,
        };
        aliases += 1;
        if aliases > MAX_CURVE_REFERENCE_DEPTH {
            return Err(Error::CurveReferenceLimitExceeded);
        }
        curve_id = next;
    };
    Ok(match geometry {
        Entity::Circle(c) => {
            let (location, axis, ref_direction) = axis2_placement_3d(s, c.position.cast());
            Curve::new_circle(location, axis, ref_direction, c.radius.0.0.0,
                              edge_curve.edge_start == edge_curve.edge_end,
                              edge_curve.same_sense ^ !orientation)
        },
        Entity::Ellipse(c) => {
            let (location, axis, ref_direction) = axis2_placement_3d(s, c.position.cast());
            Curve::new_ellipse(location, axis, ref_direction,
                               c.semi_axis_1.0.0.0, c.semi_axis_2.0.0.0,
                               edge_curve.edge_start == edge_curve.edge_end,
                               edge_curve.same_sense ^ !orientation)
        },
        Entity::BSplineCurveWithKnots(c) => {
            if c.self_intersect.0 == Some(true) {
                return Err(Error::SelfIntersectingCurve);
            }

            let control_points_list = control_points_1d(
                s, &c.control_points_list);

            let knot_vec = validated_knots(
                c.degree,
                c.knots.iter().map(|k| k.0),
                &c.knot_multiplicities,
                c.control_points_list.len(),
            )?;

            let curve = nurbs::BSplineCurve::new(
                c.closed_curve.0 != Some(true),
                knot_vec,
                control_points_list,
            );
            Curve::BSplineCurveWithKnots(SampledCurve::new(curve))
        },
        Entity::ComplexEntity(v) if v.len() == 2 => {
            let bspline = if let Entity::BSplineCurveWithKnots(b) = &v[0] {
                b
            } else {
                warn!("Could not get BSplineCurveWithKnots from {:?}", v[0]);
                return Err(Error::UnknownCurveType)
            };
            let rational = if let Entity::RationalBSplineCurve(b) = &v[1] {
                b
            } else {
                warn!("Could not get RationalBSplineCurve from {:?}", v[1]);
                return Err(Error::UnknownCurveType)
            };
            if bspline.self_intersect.0 == Some(true) {
                return Err(Error::SelfIntersectingCurve);
            }
            if rational.weights_data.len() != bspline.control_points_list.len() {
                return Err(Error::InvalidKnots);
            }
            let knot_vec = validated_knots(
                bspline.degree,
                bspline.knots.iter().map(|k| k.0),
                &bspline.knot_multiplicities,
                bspline.control_points_list.len(),
            )?;

            let control_points_list = control_points_1d(
                    s, &bspline.control_points_list)
                .into_iter()
                .zip(rational.weights_data.iter())
                .map(|(p, w)| DVec4::new(p.x * w, p.y * w, p.z * w, *w))
                .collect();

            let curve = nurbs::NURBSCurve::new(
                bspline.closed_curve.0 != Some(true),
                knot_vec,
                control_points_list,
            );
            Curve::NURBSCurve(SampledCurve::new(curve))
        },
        // The Line type ignores pnt / dir and just uses u and v
        Entity::Line(_) => Curve::new_line(),
        e => {
            warn!("Could not get edge from {:?}", e);
            return Err(Error::UnknownCurveType);
        },
    })
}

fn vertex_point(s: &StepFile, v: Vertex) -> DVec3 {
    cartesian_point(s,
        s.entity(v.cast::<VertexPoint_>())
            .expect("Could not get VertexPoint")
            .vertex_geometry
            .cast())
}
