use pyo3::prelude::*;

#[pyclass]
pub struct ItrsWrap {
    iter: PyObject,
}

impl ItrsWrap {
    pub fn with_wrap(iter: PyObject, py: Python<'_>) -> PyResult<Self> {
        let iter = iter.call_method(py, "__iter__", (), None)?;
        Ok(Self { iter })
    }
}

#[pymethods]
impl ItrsWrap {
    #[new]
    fn new(obj: &PyRawObject, py: Python<'_>, iter: PyObject) -> PyResult<()> {
        obj.init(Self::with_wrap(iter, py)?);
        Ok(())
    }
}

impl Iterator for ItrsWrap {
    type Item = PyResult<PyObject>;

    fn next(&mut self) -> Option<PyResult<PyObject>> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        match self.iter.call_method0(py, "__next__") {
            Ok(result) => Some(Ok(result)),
            Err(err) => {
                if err.is_instance::<pyo3::exceptions::StopIteration>(py) {
                    None
                } else {
                    Some(Err(err))
                }
            }
        }
    }
}

pub enum WrapResult {
    Ok(ItrsWrap),
    Err(std::iter::Once<PyErr>),
}

impl Iterator for WrapResult {
    type Item = PyResult<PyObject>;

    fn next(&mut self) -> Option<PyResult<PyObject>> {
        match self {
            WrapResult::Ok(wrap) => wrap.next(),
            WrapResult::Err(err) => err.next().map(|x| Err(x)),
        }
    }
}

impl From<PyResult<ItrsWrap>> for WrapResult {
    fn from(x: PyResult<ItrsWrap>) -> WrapResult {
        match x {
            Ok(wrap) => WrapResult::Ok(wrap),
            Err(err) => WrapResult::Err(std::iter::once(err))
        }
    }
}