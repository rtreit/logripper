//! ADIF adapter: maps between `difa::Record` and proto `QsoRecord`.

pub mod mapper;

pub use mapper::AdifMapper;
