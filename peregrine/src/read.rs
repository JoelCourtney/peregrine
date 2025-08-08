pub trait Readable: Send + Sync + 'static {
    type Read: Copy + Send;

    fn read(&self) -> Self::Read;
}
