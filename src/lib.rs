use pyo3::prelude::*;

mod itrs;
use itrs::Itrs;
mod wrap;
use itrs::PeekItrs;

/// This module is a python module implemented in Rust.
#[pymodule]
fn itrs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Itrs>()?;
    m.add_class::<PeekItrs>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
