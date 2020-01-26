//! Implement iterators methods as python methods on Itrs

use pyo3::prelude::*;
use pyo3::types::PyTuple;

use super::peek::PeekItrs;
use crate::wrap::{ItrsWrap, WrapResult};

use super::Itrs;

#[pymethods]
impl Itrs {
    fn next(&mut self) -> PyResult<Option<PyObject>> {
        let mut inner = self.as_it()?;
        Iterator::next(&mut *inner).transpose()
    }

    fn count(&mut self) -> PyResult<usize> {
        let mut inner = self.as_it()?;
        Ok(Iterator::count(&mut *inner))
    }

    fn last(&mut self) -> PyResult<Option<PyObject>> {
        let mut inner = self.as_it()?;
        Iterator::last(&mut *inner).transpose()
    }

    fn nth(&mut self, n: usize) -> PyResult<Option<PyObject>> {
        let mut inner = self.as_it()?;
        Iterator::nth(&mut *inner, n).transpose()
    }

    fn step_by(&mut self, step: usize) -> PyResult<Self> {
        Ok(Self::from_it(Iterator::step_by(self.clone_inner(), step)))
    }

    fn chain(&mut self, other: &mut Self) -> Self {
        Self::from_it(Iterator::chain(self.clone_inner(), other.clone_inner()))
    }

    fn zip(&mut self, other: &mut Self) -> Self {
        let it = Iterator::zip(self.clone_inner(), other.clone_inner());
        Self::from_it(it.map(move |(x, y)| {
            let x = x?;
            let y = y?;
            let gil = Python::acquire_gil();
            let py = gil.python();
            Ok(PyTuple::new(py, [x, y].into_iter()).to_object(py))
        }))
    }

    fn map(&mut self, f: PyObject) -> Self {
        let f = move |x| {
            let x = x?;
            let gil = Python::acquire_gil();
            let py = gil.python();
            f.call1(py, PyTuple::new(py, Some(x)))
        };
        Self::from_it(Iterator::map(self.clone_inner(), f))
    }

    // fn try_for_each(&mut self, f: PyObject) -> PyResult<()>
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
        Iterator::for_each(self.clone_inner(), f);
        match err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn filter(&mut self, f: PyObject) -> Self {
        Self::from_it(Iterator::filter(
            self.clone_inner(),
            move |x: &PyResult<PyObject>| {
                // Do not filter errors, they will be encountered when actually running the iterator
                apply_filter(&f, x).unwrap_or(true)
            },
        ))
    }

    fn filter_map(&mut self, f: PyObject) -> Self {
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
        Self::from_it(Iterator::filter_map(self.clone_inner(), f))
    }

    fn enumerate(&mut self) -> Self {
        Self::from_it(
            Iterator::enumerate(self.clone_inner()).map(|(index, value)| {
                let value = value?;

                let gil = Python::acquire_gil();
                let py = gil.python();
                Ok(PyTuple::new(py, [index.to_object(py), value].into_iter()).to_object(py))
            }),
        )
    }

    fn peekable(&mut self) -> PeekItrs {
        let peek = PeekItrs::from_iterator(self.clone_inner());
        self.inner = peek.itrs.clone_inner();
        peek
    }

    fn skip_while(&mut self, predicate: PyObject) -> Self {
        Self::from_it(Iterator::skip_while(
            self.clone_inner(),
            move |x: &PyResult<PyObject>| {
                // We do **not** want to skip any potential error, so err => false
                apply_filter(&predicate, x).unwrap_or(false)
            },
        ))
    }

    fn take_while(&mut self, predicate: PyObject) -> Self {
        Self::from_it(Iterator::take_while(
            self.clone_inner(),
            move |x: &PyResult<PyObject>| {
                // we **do** want to take any potential error, so err => true
                apply_filter(&predicate, x).unwrap_or(true)
            },
        ))
    }

    fn skip(&mut self, n: usize) -> Self {
        Self::from_it(Iterator::skip(self.clone_inner(), n))
    }

    fn take(&mut self, n: usize) -> Self {
        Self::from_it(Iterator::take(self.clone_inner(), n))
    }

    /// f is a function with the following definition:
    /// fn f(initial_state, iterator_element) -> (new_state, transformed_element)
    fn scan(&mut self, initial_state: PyObject, f: PyObject) -> Self {
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

        Self::from_it(Iterator::scan(
            self.clone_inner(),
            initial_state,
            move |state: &mut PyObject, item: PyResult<PyObject>| {
                transform_option(scan_closure(state, item))
            },
        ))
    }

    fn flat_map(&mut self, f: PyObject) -> Self {
        let f = move |item: PyResult<PyObject>| -> PyResult<ItrsWrap> {
            let item = item?;
            let gil = Python::acquire_gil();
            let py = gil.python();

            ItrsWrap::with_wrap(f.call1(py, PyTuple::new(py, Some(item)))?, py)
        };

        Self::from_it(Iterator::flat_map(self.clone_inner(), move |x| {
            WrapResult::from(f(x))
        }))
    }

    fn flatten(&mut self) -> Self {
        let f = move |item: PyResult<PyObject>| -> PyResult<ItrsWrap> {
            let item = item?;
            let gil = Python::acquire_gil();
            let py = gil.python();

            ItrsWrap::with_wrap(item, py)
        };

        Self::from_it(Iterator::flat_map(self.clone_inner(), move |x| {
            WrapResult::from(f(x))
        }))
    }

    fn fuse(&mut self) -> Self {
        Self::from_it(Iterator::fuse(self.clone_inner()))
    }

    fn inspect(&mut self, f: PyObject) -> Self {
        let f = move |x: &PyResult<PyObject>| -> PyResult<()> {
            let gil = Python::acquire_gil();
            let py = gil.python();
            let x = x.as_ref().map_err(|err| err.clone_ref(py))?;

            f.call1(py, PyTuple::new(py, Some(x)))?;
            Ok(())
        };

        Self::from_it(Iterator::map(
            self.clone_inner(),
            move |x: PyResult<PyObject>| -> PyResult<PyObject> {
                f(&x)?;
                x
            },
        ))
    }

    // fn collect(&mut self, ty: PyObject) -> PyResult<PyObject> {}
    // fn partition(&mut self, ty: PyObject, f: PyObject) -> PyResult<(PyObject, PyObject)> {}

    // fn try_fold(&mut self, initial_value: PyObject, f: PyObject) -> PyResult<PyObject> {}
    fn fold(&mut self, initial_value: PyObject, f: PyObject) -> PyResult<PyObject> {
        let f = |accumulator, x: PyResult<PyObject>| {
            let gil = Python::acquire_gil();
            let py = gil.python();
            x.and_then(|x| f.call1(py, PyTuple::new(py, [accumulator, x].into_iter())))
        };
        Iterator::try_fold(&mut self.clone_inner(), initial_value, f)
    }

    // TODO: lots of missing combinators
    // fn all<F>(&mut self, f: F) -> bool
    // fn any<F>(&mut self, f: F) -> bool
    // fn find<P>(&mut self, predicate: P) -> Option<Self::Item>
    // fn find_map<B, F>(&mut self, f: F) -> Option<B>
    // fn position<P>(&mut self, predicate: P) -> Option<usize>
    // fn rposition<P>(&mut self, predicate: P) -> Option<usize>
    // fn max(self) -> Option<Self::Item>
    // fn min(self) -> Option<Self::Item>
    // fn max_by_key<B, F>(self, f: F) -> Option<Self::Item>
    // fn max_by<F>(self, compare: F) -> Option<Self::Item>
    // fn min_by_key<B, F>(self, f: F) -> Option<Self::Item>
    // fn min_by<F>(self, compare: F) -> Option<Self::Item>
    // fn unzip<A, B, FromA, FromB>(self) -> (FromA, FromB)
    // fn cycle(self) -> Cycle<Self>


    // FIXME: this cheats, as this doesn't use Rust's sum method.
    // I tried using it previously, but it involves implementing the std::iter::Sum trait
    // for a struct containing a PyResult<PyObject> and a PyObject playing the role of the "zero".
    // I elected to use the following simple implementation instead for the moment.
    fn sum(&mut self, py: Python<'_>, first: PyObject) -> PyResult<PyObject> {
        let mut accumulator = first;
        for pyobject in &mut *(self.as_it()?) {
            let pyobject = pyobject?;
            accumulator =
                accumulator.call_method1(py, "__add__", PyTuple::new(py, Some(pyobject)))?
        }
        Ok(accumulator)
    }

    // fn product<P>(self) -> P
    // fn cmp<I>(self, other: I) -> Ordering
    // fn cmp_by<I, F>(self, other: I, cmp: F) -> Ordering
    // fn partial_cmp<I>(self, other: I) -> Option<Ordering>
    // fn partial_cmp_by<I, F>(self, other: I, partial_cmp: F) -> Option<Ordering>
    // fn eq<I>(self, other: I) -> bool
    // fn eq_by<I, F>(self, other: I, eq: F) -> bool
    // fn ne<I>(self, other: I) -> bool
    // fn lt<I>(self, other: I) -> bool
    // fn le<I>(self, other: I) -> bool
    // fn gt<I>(self, other: I) -> bool
    // fn ge<I>(self, other: I) -> bool
    // fn is_sorted(self) -> bool // NIGHTLY ONLY
    // fn is_sorted_by<F>(self, compare: F) -> bool // NIGHTLY ONLY
    // fn is_sorted_by_key<F, K>(self, f: F) -> bool // NIGHTLY ONLY
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
