use std::ffi::CStr;
use std::fs::File;
use std::io::Read;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use step::step_file::{ParseError, StepFile};
use triangulate::mesh::Mesh;
use triangulate::triangulate::triangulate;

const MAX_STEP_BYTES: u64 = 128 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoxtrotLoadStatus {
    FoxtrotLoadSuccess = 0,
    FoxtrotLoadUnreadable = 1,
    FoxtrotLoadTooLarge = 2,
    FoxtrotLoadInvalid = 3,
    FoxtrotLoadNoGeometry = 4,
    FoxtrotLoadGeometryLimit = 5,
}

use FoxtrotLoadStatus::*;

fn load_step_mesh(path: &str) -> Result<(Mesh, usize), FoxtrotLoadStatus> {
    let file = File::open(path).map_err(|_| FoxtrotLoadUnreadable)?;
    let metadata = file.metadata().map_err(|_| FoxtrotLoadUnreadable)?;
    if !metadata.is_file() {
        return Err(FoxtrotLoadUnreadable);
    }
    if metadata.len() > MAX_STEP_BYTES {
        return Err(FoxtrotLoadTooLarge);
    }

    let mut data = Vec::new();
    file.take(MAX_STEP_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| FoxtrotLoadUnreadable)?;
    if data.len() as u64 > MAX_STEP_BYTES {
        return Err(FoxtrotLoadTooLarge);
    }

    let flat = StepFile::strip_flatten(&data);
    drop(data);
    let entities = StepFile::parse(&flat).map_err(|error| match error {
        ParseError::InvalidDataSection => FoxtrotLoadInvalid,
        ParseError::TooManyRecords => FoxtrotLoadGeometryLimit,
    })?;
    if entities.0.is_empty() {
        return Err(FoxtrotLoadInvalid);
    }
    let (mesh, stats) = triangulate(&entities).map_err(|error| match error {
        triangulate::Error::RepresentationLimitExceeded | triangulate::Error::MeshLimitExceeded => {
            FoxtrotLoadGeometryLimit
        }
        _ => FoxtrotLoadInvalid,
    })?;

    if mesh.verts.is_empty() || mesh.triangles.is_empty() {
        return Err(FoxtrotLoadNoGeometry);
    }

    Ok((mesh, stats.num_errors + stats.num_panics))
}

#[repr(C)]
pub struct MeshSlice {
    pub verts: *const f32,
    pub normals: *const f32,
    pub tris: *const u32,
    pub vert_count: usize,
    pub tri_count: usize,
    pub omitted_faces: usize,
}

fn flatten_mesh(mesh: Mesh, omitted_faces: usize) -> Result<MeshSlice, FoxtrotLoadStatus> {
    let vert_count = mesh.verts.len();
    let tri_count = mesh.triangles.len();
    let mut vertices = Vec::with_capacity(vert_count * 3);
    let mut normals = Vec::with_capacity(vert_count * 3);
    let mut indices = Vec::with_capacity(tri_count * 3);

    for v in mesh.verts {
        let position = [v.pos.x as f32, v.pos.y as f32, v.pos.z as f32];
        if !position.iter().all(|coordinate| coordinate.is_finite()) {
            return Err(FoxtrotLoadInvalid);
        }
        vertices.extend(position);
        normals.extend([v.norm.x as f32, v.norm.y as f32, v.norm.z as f32]);
    }
    for t in mesh.triangles {
        if t.verts.iter().any(|&index| index as usize >= vert_count) {
            return Err(FoxtrotLoadInvalid);
        }
        indices.extend(t.verts.iter().copied());
    }

    // Transfer fixed-length slices so release does not have to infer Vec capacity.
    Ok(MeshSlice {
        verts: Box::into_raw(vertices.into_boxed_slice()) as *const f32,
        normals: Box::into_raw(normals.into_boxed_slice()) as *const f32,
        tris: Box::into_raw(indices.into_boxed_slice()) as *const u32,
        vert_count,
        tri_count,
        omitted_faces,
    })
}

/// On success, `out_mesh` receives three owned buffers. Release once with
/// `foxtrot_free_mesh`. On failure, `out_mesh` is left untouched.
///
/// # Safety
/// `path` must point to a NUL-terminated C string and `out_mesh` must be writable.
#[no_mangle]
pub unsafe extern "C" fn foxtrot_load_step(
    path: *const c_char,
    out_mesh: *mut MeshSlice,
) -> FoxtrotLoadStatus {
    if path.is_null() || out_mesh.is_null() {
        return FoxtrotLoadInvalid;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let path = unsafe { CStr::from_ptr(path) }
            .to_str()
            .map_err(|_| FoxtrotLoadUnreadable)?;
        let (mesh, omitted_faces) = load_step_mesh(path)?;
        flatten_mesh(mesh, omitted_faces)
    }));
    match result {
        Ok(Ok(mesh)) => {
            unsafe { out_mesh.write(mesh) };
            FoxtrotLoadSuccess
        }
        Ok(Err(status)) => status,
        Err(_) => FoxtrotLoadInvalid,
    }
}

/// # Safety
/// Each buffer must be a still-owned allocation returned by `foxtrot_load_step`.
#[no_mangle]
pub unsafe extern "C" fn foxtrot_free_mesh(slice: MeshSlice) {
    unsafe {
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
            slice.verts as *mut f32,
            slice.vert_count * 3,
        )));
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
            slice.normals as *mut f32,
            slice.vert_count * 3,
        )));
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
            slice.tris as *mut u32,
            slice.tri_count * 3,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::path::Path;

    #[test]
    fn loads_and_releases_sample_meshes() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../foxtrot/examples");
        for (name, vertices, triangles) in [
            ("cuboid.step", 24, 12),
            ("cube_hole.step", 276, 268),
        ] {
            let path = fixtures.join(name);
            let path = CString::new(path.to_str().unwrap()).unwrap();
            let mut output = MaybeUninit::uninit();
            let result = unsafe { foxtrot_load_step(path.as_ptr(), output.as_mut_ptr()) };
            assert_eq!(result, FoxtrotLoadSuccess, "{name}");

            let mesh = unsafe { output.assume_init() };
            let counts = (mesh.vert_count, mesh.tri_count);
            unsafe { foxtrot_free_mesh(mesh) };
            assert_eq!(counts, (vertices, triangles), "{name}");
        }
    }

    #[test]
    fn reports_an_unreadable_file() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("nonexistent-test-file.step");
        let path = CString::new(path.to_str().unwrap()).unwrap();
        let mut output = MaybeUninit::uninit();
        let result = unsafe { foxtrot_load_step(path.as_ptr(), output.as_mut_ptr()) };
        assert_eq!(result, FoxtrotLoadUnreadable);
    }
}
