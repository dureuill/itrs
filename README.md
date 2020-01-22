# Itrs

Rust's iterators in python, implemented in Rust!

`Itrs` is a python native module that exposes to python an `Iterator` interface similar to Rust's [`Iterator` trait](https://doc.rust-lang.org/std/iter/trait.Iterator.html).

This means that, given the following innocent fibonacci generator:

```py
>>> def fibo(): 
...     yield 1 
...     yield 2 
...     current, next = 1, 2 
...     while True: 
...         current, next = next, current + next 
...         yield next 
...
```

instead of the following python code:

```py
>>> import itertools
>>> # Euler problem #2
>>> sum(x for x in itertools.takewhile(lambda x: x < 4_000_000, fibo()) if x % 2 == 0)
4613732
```

you can now write the following:

```py
>>> from itrs import Itrs
>>> (Itrs(fibo()). 
...   take_while(lambda x: x < 4_000_000). 
...   filter(lambda x: x % 2 == 0). 
...   sum(0) 
... ) 
4613732
```

## But why, you madman?


I dislike python's default iterator syntax that uses functions as combinators, because it becomes very difficult to read when using several of them. Some useful combinators and folding functions (such as `count`) are also missing, or available only in itertools.

A second reason is as an exercice in using [PyO3](https://github.com/PyO3/pyo3) and rust's `Iterator` trait. This also allowed me to play with `Result` and `Option` types so I could get the type I wanted in all situations.

Also, because I'm a madman 🤪!

## How do I use this?

### From PyPI

Build from PyPI is only supported for x64 Linux at the moment.

Using ~~cargo~~ your favorite python installation method from PyPI:

```bash
$ pip install itrs
```

Then, from the virtualenv in which you --obviously-- executed the previous command, open a python interpreter and:

```py
>>> from itrs import Itrs
>>>
>>> # you can create Itrs objects from any iterable
>>> it_array = Itrs([0, 1, 2])
>>> it_str = Itrs("Intel the Beagle")
>>> def i_yield():
>>>    for i in range(10):
>>>        yield i
>>> it_yield = Itrs(i_yield())
>>> 
>>> # you can iterate on any itrs object using normal python syntax
>>> for elem in it_array:
>>>     print(elem)
0
1
2
>>> # Iterating a second time on a exhausted iterator yields no further result
>>> for elem in it_array:
>>>     print(elem)

>>> # you can call methods on Itrs objects to create new iterators or produce results.
>>> # That's the whole selling point of this!
>>> it_str.filter(lambda x: x == ' ').count()
2
>>> [x for x in it_yield.skip(1).filter(lambda x: x % 2 == 0).map(lambda x: x * x)]
[4, 16, 36, 64]
```

### Compiling the library from Rust

Compiling from source *should* work with Linux, Windows or OSX indifferently, but was only tested under Linux.
As PyO3 requires nightly at the moment, a nightly toolchain of Rust is required to compile this repository.

The repository contains a `rust-toolchain` file containing a nightly toolchain that is known to work with the project.

If using `rustup`, you can install that toolchain with:

```bash
$ rustup toolchain install nightly-2019-09-04
```

Then, after cloning the repository, just:

```bash
$ cargo build --release
```

After the build completes, you will need to rename the produced binary by dropping the `lib` prefix, so it can be imported by python as intended:

```bash
$ mv target/release/{lib,}itrs.so
```

Alternatively, you can also build a wheel using [maturin](https://github.com/PyO3/maturin)

```bash
$ pip install maturin
$ maturin build --release
```

and then install the wheel with:

```bash
$ pip install target/wheels/itrs-0.1.0-cp38-cp38-manylinux1_x86_64.whl
```

You're done! You can now import `itrs` from a python shell with the `target/release` directory in your python path.

## Should I use this in production?

No.

* Even if python iterators suck, they are the standard, and using an external library to reimplement iterators should trigger a "**WAT?**" reaction in everyone around you
* Using python iterators, you can write new function combinators. Adding new combinators to `Itrs` would involve monkey patching, and is not possible since `Itrs` is an extension type. Note that in rust, new combinators can be added using method syntax thanks to the trait system. This is what the [itertools crate](https://docs.rs/itertools/0.8.2/itertools/index.html) does, for example.
* BTW, all iterator combinators aren't added as of yet.
* Performance is bad. There are several reasons for this:
    * Rust uses monomorphization of generic type parameters. This avoid the indirection of runtime polymorphism, and allows to inline the iterator code, which can then enable efficient optimizations at compile time. However, Python being an interpreted language without generics,it cannot take advantage of monomorphization, so the current design performs one allocation per combinator, which is far from efficient.
    * The current implementation is naive and only attempts to reuse the functions defined by the `Iterator` trait as much as possible. It also uses `Rc` rather than PyO3's provided `PyObject`, which might be inefficient considering we're already in the python runtime?
