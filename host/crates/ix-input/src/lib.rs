//! Virtual stylus / touch / keyboard injection. Real impl in Plan 8.

#[derive(Debug, Clone, Copy)]
pub struct StylusSample {
    pub x: f32, pub y: f32, pub pressure: f32, pub tilt_x: f32, pub tilt_y: f32,
}

pub trait StylusSink: Send {
    fn submit(&mut self, _sample: StylusSample) -> Result<(), std::io::Error> { Ok(()) }
}
