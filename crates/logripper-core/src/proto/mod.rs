//! Re-export generated protobuf types.

pub mod logripper {
    pub mod domain {
        tonic::include_proto!("logripper.domain");
    }
    pub mod services {
        tonic::include_proto!("logripper.services");
    }
}
