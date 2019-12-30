//! Implement `PeekItrs` for the `peekable()` method on `Itrs`

use pyo3::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use super::{Itrs, RefIterator};
use crate::wrap::ItrsWrap;

#[pyclass(freelist = 100)]
pub(crate) struct PeekItrs {
    pub(in crate::itrs) peek: PeekIterator,
    pub(in crate::itrs) itrs: Itrs,
}

impl PeekItrs {
    pub(in crate::itrs) fn from_iterator(iter: RefIterator) -> Self {
        let peek = iter.peekable();
        let peek = Rc::new(RefCell::new(peek));
        let itrs = Itrs {
            inner: RefIterator(peek.clone()),
        };

        let peek = PeekIterator(peek);

        Self { peek, itrs }
    }
}

#[pymethods]
impl PeekItrs {
    #[new]
    fn new(obj: &PyRawObject, py: Python<'_>, iter: PyObject) -> PyResult<()> {
        let peek = RefIterator(Rc::new(RefCell::new(ItrsWrap::with_wrap(
            iter.clone_ref(py),
            py,
        )?)));

        obj.init(Self::from_iterator(peek));
        Ok(())
    }

    fn peek(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        self.peek
            .0
            .try_borrow_mut()
            .map_err(|_| pyo3::exceptions::ValueError::py_err("Already borrowed iterator"))?
            .peek()
            .map(|result| result.as_ref())
            .transpose()
            .map(|obj| obj.map(|obj| obj.clone_ref(py)))
            .map_err(|err| err.clone_ref(py))
    }

    fn to_itrs(&self) -> Itrs {
        self.itrs.clone()
    }
}

#[derive(Clone)]
pub(in crate::itrs) struct PeekIterator(Rc<RefCell<std::iter::Peekable<RefIterator>>>);
