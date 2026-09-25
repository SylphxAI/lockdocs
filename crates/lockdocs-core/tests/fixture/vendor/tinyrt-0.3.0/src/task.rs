cfg_rt! {
    /// Spawns a future onto the runtime and returns its handle.
    pub fn spawn<F: Future>(future: F) -> JoinHandle<F::Output> {
        todo!()
    }
}
