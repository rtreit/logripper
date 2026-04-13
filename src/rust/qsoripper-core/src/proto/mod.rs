//! Re-export generated protobuf types.

pub mod qsoripper {
    //! Generated `qsoripper` protobuf packages.

    /// Generated domain messages and enums.
    // Generated prost/tonic code is not edited by hand; keep lint signal focused on project code.
    #[allow(missing_docs, clippy::all, clippy::pedantic)]
    pub mod domain {
        tonic::include_proto!("qsoripper.domain");
    }
    /// Generated gRPC service clients and servers.
    // Generated prost/tonic code is not edited by hand; keep lint signal focused on project code.
    #[allow(missing_docs, clippy::all, clippy::pedantic)]
    pub mod services {
        tonic::include_proto!("qsoripper.services");
    }
}
