use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::PyIterProtocol;

mod wrap;
use wrap::{ItrsWrap, WrapResult};

/// This module is a python module implemented in Rust.
#[pymodule]
fn itrs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Itrs>()?;
    Ok(())
}

#[pyclass]
struct Itrs {
    inner: Option<Box<dyn Iterator<Item = PyResult<PyObject>>>>,
}

impl Itrs {
    fn from_it<It>(inner: It) -> Self
    where
        It: Iterator<Item = PyResult<PyObject>> + 'static,
    {
        Self {
            inner: Some(Box::new(inner)),
        }
    }

    fn as_it(&mut self) -> PyResult<&mut (dyn Iterator<Item = PyResult<PyObject>> + 'static)> {
        self.inner
            .as_mut()
            .map(|x| x.as_mut())
            .ok_or_else(|| pyo3::exceptions::ValueError::py_err("Moved-out iterator"))
    }

    fn take_box(&mut self) -> PyResult<Box<dyn Iterator<Item = PyResult<PyObject>>>> {
        self.inner
            .take()
            .ok_or_else(|| pyo3::exceptions::ValueError::py_err("Moved-out iterator"))
    }
}

#[pymethods]
impl Itrs {
    #[new]
    fn new(obj: &PyRawObject, py: Python<'_>, iter: PyObject) -> PyResult<()> {
        obj.init(Self {
            inner: Some(Box::new(ItrsWrap::with_wrap(iter, py)?)),
        });
        Ok(())
    }

    fn next(&mut self) -> PyResult<Option<PyObject>> {
        let inner = self.as_it()?;
        Iterator::next(inner).transpose()
    }

    fn count(&mut self) -> PyResult<usize> {
        let inner = self.as_it()?;
        Ok(Iterator::count(inner))
    }

    fn last(&mut self) -> PyResult<Option<PyObject>> {
        let inner = self.as_it()?;
        Iterator::last(inner).transpose()
    }

    fn nth(&mut self, n: usize) -> PyResult<Option<PyObject>> {
        let inner = self.as_it()?;
        Iterator::nth(inner, n).transpose()
    }

    fn step_by(&mut self, step: usize) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::step_by(self.take_box()?, step)))
    }

    fn chain(&mut self, other: &mut Self) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::chain(
            self.take_box()?,
            other.take_box()?,
        )))
    }

    fn zip(&mut self, other: &mut Self) -> PyResult<Self> {
        let it = Iterator::zip(self.take_box()?, other.take_box()?);
        Ok(Self::from_it(it.map(move |(x, y)| {
            let x = x?;
            let y = y?;
            let gil = Python::acquire_gil();
            let py = gil.python();
            Ok(PyTuple::new(py, [x, y].into_iter()).to_object(py))
        })))
    }

    fn map(&mut self, f: PyObject) -> PyResult<Self> {
        let f = move |x| {
            let x = x?;
            let gil = Python::acquire_gil();
            let py = gil.python();
            f.call1(py, PyTuple::new(py, Some(x)))
        };
        Ok(Self::from_it(Iterator::map(self.take_box()?, f)))
    }

    fn for_each(&mut self, f: PyObject) -> PyResult<()> {
        let mut err = None;
        let f = |x: PyResult<PyObject>| {
            let gil = Python::acquire_gil();
            let py = gil.python();
            match x.and_then(|x| f.call1(py, PyTuple::new(py, Some(x)))) {
                Ok(_) => {}
                Err(e) => err = Some(e),
            };
        };
        Iterator::for_each(self.take_box()?, f);
        match err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn filter(&mut self, f: PyObject) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::filter(
            self.take_box()?,
            move |x: &PyResult<PyObject>| {
                // Do not filter errors, they will be encountered when actually running the iterator
                apply_filter(&f, x).unwrap_or(true)
            },
        )))
    }

    fn filter_map(&mut self, f: PyObject) -> PyResult<Self> {
        let f = move |x: PyResult<PyObject>| {
            let gil = Python::acquire_gil();
            let py = gil.python();
            let g = || {
                let x = x?;
                f.call1(py, PyTuple::new(py, Some(x)))
            };

            match g() {
                Ok(res) => {
                    if res.is_none() {
                        None
                    } else {
                        Some(Ok(res))
                    }
                }
                Err(err) => Some(Err(err)),
            }
        };
        Ok(Self::from_it(Iterator::filter_map(self.take_box()?, f)))
    }

    fn enumerate(&mut self) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::enumerate(self.take_box()?).map(
            |(index, value)| {
                let value = value?;

                let gil = Python::acquire_gil();
                let py = gil.python();
                // TODO: check zip
                Ok(PyTuple::new(py, Some(index).into_iter().zip(Some(value))).to_object(py))
            },
        )))
    }

    // TODO
    // fn peekable(&mut self);

    fn skip_while(&mut self, predicate: PyObject) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::skip_while(
            self.take_box()?,
            move |x: &PyResult<PyObject>| {
                // We do **not** want to skip any potential error, so err => false
                apply_filter(&predicate, x).unwrap_or(false)
            },
        )))
    }

    fn take_while(&mut self, predicate: PyObject) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::take_while(
            self.take_box()?,
            move |x: &PyResult<PyObject>| {
                // we **do** want take any potential error, so err => true
                apply_filter(&predicate, x).unwrap_or(true)
            },
        )))
    }

    fn skip(&mut self, n: usize) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::skip(self.take_box()?, n)))
    }

    fn take(&mut self, n: usize) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::take(self.take_box()?, n)))
    }

    /// f in a function with the following definition:
    /// fn f(initial_state, iterator_element) -> (new_state, transformed_element)
    fn scan(&mut self, initial_state: PyObject, f: PyObject) -> PyResult<Self> {
        // Call the passed closure and recover its return value to update the state and produce the next element
        let scan_closure =
            move |state: &mut PyObject, item: PyResult<PyObject>| -> PyResult<PyObject> {
                let item = item?;
                let gil = Python::acquire_gil();
                let py = gil.python();
                let tuple = f.call1(
                    py,
                    PyTuple::new(py, [state.clone_ref(py), item].into_iter()),
                )?;
                let tuple = tuple.cast_as::<PyTuple>(py)?;
                let (new_state, transformed_element) = get_scan_pair(tuple)?;
                *state = new_state.clone_ref(py);
                Ok(transformed_element.clone_ref(py))
            };

        // Map errors to Some (so they are not ignored), and PythonNone to None (to stop iteration)
        let transform_option = |result: PyResult<PyObject>| match result {
            Ok(obj) => {
                if obj.is_none() {
                    None
                } else {
                    Some(Ok(obj))
                }
            }
            Err(err) => Some(Err(err)),
        };

        Ok(Self::from_it(Iterator::scan(
            self.take_box()?,
            initial_state,
            move |state: &mut PyObject, item: PyResult<PyObject>| {
                transform_option(scan_closure(state, item))
            },
        )))
    }

    fn flat_map(&mut self, f: PyObject) -> PyResult<Self> {
        let f = move |item: PyResult<PyObject>| -> PyResult<ItrsWrap> {
            let item = item?;
            let gil = Python::acquire_gil();
            let py = gil.python();

            ItrsWrap::with_wrap(f.call1(py, PyTuple::new(py, Some(item)))?, py)
        };

        Ok(Self::from_it(Iterator::flat_map(
            self.take_box()?,
            move |x| WrapResult::from(f(x)),
        )))
    }

    fn flatten(&mut self) -> PyResult<Self> {
        let f = move |item: PyResult<PyObject>| -> PyResult<ItrsWrap> {
            let item = item?;
            let gil = Python::acquire_gil();
            let py = gil.python();

            ItrsWrap::with_wrap(item, py)
        };

        Ok(Self::from_it(Iterator::flat_map(
            self.take_box()?,
            move |x| WrapResult::from(f(x)),
        )))
    }

    fn fuse(&mut self) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::fuse(self.take_box()?)))
    }

    fn inspect(&mut self, f: PyObject) -> PyResult<Self> {
        let f = move |x: &PyResult<PyObject>| -> PyResult<()> {
            let gil = Python::acquire_gil();
            let py = gil.python();
            let x = x.as_ref().map_err(|err| err.clone_ref(py))?;

            f.call1(py, PyTuple::new(py, Some(x)))?;
            Ok(())
        };

        Ok(Self::from_it(Iterator::map(
            self.take_box()?,
            move |x: PyResult<PyObject>| -> PyResult<PyObject> {
                f(&x)?;
                x
            },
        )))
    }

}

fn apply_filter(predicate: &PyObject, x: &PyResult<PyObject>) -> Result<bool, ()> {
    let gil = Python::acquire_gil();
    let py = gil.python();

    let x = x.as_ref().map_err(|_| ())?;
    predicate
        .call1(py, PyTuple::new(py, [x].into_iter()))
        .map_err(|_| ())?
        .is_true(py)
        .map_err(|_| ())
}

fn get_scan_pair(tuple: &PyTuple) -> PyResult<(&PyObject, &PyObject)> {
    let tuple = tuple.as_slice();
    let type_error = || {
        pyo3::exceptions::TypeError::py_err(
            "Expected tuple (new_state, transformed_element) as return from passed function",
        )
    };
    let new_state = tuple.get(0).ok_or_else(type_error)?;
    let transformed_element = tuple.get(1).ok_or_else(type_error)?;
    Ok((new_state, transformed_element))
}

#[pyproto]
impl PyIterProtocol for Itrs {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<Itrs>> {
        Ok(slf.into())
    }

    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        let inner = slf.as_it()?;

        inner.next().transpose()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
