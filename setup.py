from setuptools import setup
from setuptools_rust import Binding, RustExtension

setup(
    name="heartbeat",
    version="1.0",
    rust_extensions=[RustExtension("py.py", binding=Binding.PyO3)],
    packages=["."],
    # rust extensions are not zip safe, just like C-extensions.
    zip_safe=False,
)
