pub struct StreamSource {
    /// sample rate initialized with
    claimed_sample_rate: f64,

    /// `Instant` that the `Sink` started at
    frame_count: u64,

    incoming: Vec<rtrb::Consumer<f32>>,
    last_frames: Vec<[f32; FRAME_LOOKBACK]>,

    /// an estimate of where the device's buffer is at time-wise
    estimated_buffer_time: Duration,
    /// an estimate of how much ahead the device's buffer is, relative to what is
    /// currently playing
    estimated_buffer_ahead: Option<Duration>,

    strategy: CompensationStrategy,
    /// in frames
    compensation_start_threshold: f64,
}
