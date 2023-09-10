use super::Inject;

impl Inject {
    pub(crate) fn len(&self) -> usize {
        self.shared.len()
    }
}
