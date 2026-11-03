use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("{0}")]
    Gpu(#[from] gpu::GpuError),
    #[error("{0}")]
    Compositor(#[from] compositor::CompositorError),
    #[error("{0}")]
    Media(#[from] media::MediaError),
    #[error("media {0} is not in the project")]
    MissingMedia(String),
}
