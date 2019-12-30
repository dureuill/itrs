//! Implements the `Itrs`, main type of the library

use pyo3::prelude::*;
use pyo3::PyIterProtocol;

use std::cell::RefCell;
use std::rc::Rc;

use crate::wrap::ItrsWrap;

mod it_fn;
mod peek;

pub(crate) use peek::PeekItrs;

#[pyclass(freelist = 100)]
#[derive(Clone)]
pub(crate) struct Itrs {
    inner: RefIterator,
}

#[derive(Clone)]
struct RefIterator(Rc<RefCell<dyn Iterator<Item = PyResult<PyObject>>>>);

impl Iterator for RefIterator {
    type Item = PyResult<PyObject>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .try_borrow_mut()
            .map(|mut it| it.next())
            .unwrap_or_else(|_| {
                Some(Err(pyo3::exceptions::RuntimeError::py_err(
                    "Already borrowed iterator",
                )))
            })
    }
}

impl Itrs {
    fn from_it<It>(inner: It) -> Self
    where
        It: Iterator<Item = PyResult<PyObject>> + 'static,
    {
        Self {
            inner: RefIterator(Rc::new(RefCell::new(inner))),
        }
    }

    fn as_it(
        &mut self,
    ) -> PyResult<std::cell::RefMut<(dyn Iterator<Item = PyResult<PyObject>> + 'static)>> {
        self.inner
            .0
            .try_borrow_mut()
            .map_err(|_| pyo3::exceptions::ValueError::py_err("Already borrowed iterator"))
    }

    fn clone_inner(&self) -> RefIterator {
        RefIterator(Rc::clone(&self.inner.0))
    }
}

#[pymethods]
impl Itrs {
    #[new]
    fn new(obj: &PyRawObject, py: Python<'_>, iter: PyObject) -> PyResult<()> {
        obj.init(Self {
            inner: RefIterator(Rc::new(RefCell::new(ItrsWrap::with_wrap(iter, py)?))),
        });
        Ok(())
    }

    fn get_ref_count(&self) -> usize {
        Rc::strong_count(&self.inner.0)
    }
}

#[pyproto]
impl PyIterProtocol for Itrs {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<Itrs>> {
        Ok(slf.into())
    }

    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        let mut inner = slf.as_it()?;

        inner.next().transpose()
    }
}
