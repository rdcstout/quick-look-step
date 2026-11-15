pub mod mesh;
pub mod stats;
pub mod surface;
pub mod triangulate;
pub mod curve;

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum Error {
    #[error("The STEP assembly references a representation that is already on the current path")]
    CyclicRepresentation,

    #[error("The STEP assembly exceeds the supported representation count")]
    RepresentationLimitExceeded,

    #[error("The STEP mesh exceeds the supported vertex or triangle count")]
    MeshLimitExceeded,

    #[error("The STEP curve reference chain is cyclic or too deep")]
    CurveReferenceLimitExceeded,

    #[error("The STEP spline has inconsistent knots, multiplicities, or control points")]
    InvalidKnots,

    #[error("Could not lower point to 2D for triangulation")]
    CouldNotLower,

    #[error("Could not convert into a Surface")]
    UnknownSurfaceType,

    #[error("Could not convert into a Curve")]
    UnknownCurveType,

    #[error("Closed NURBS and b-spline surfaces are not implemented")]
    ClosedSurface,

    #[error("Self-intersecting NURBS and b-spline surfaces are not implemented")]
    SelfIntersectingSurface,

    #[error("Closed NURBS and b-spline curves are not implemented")]
    ClosedCurve,

    #[error("Self-intersecting NURBS and b-spline curves are not implemented")]
    SelfIntersectingCurve,
}
