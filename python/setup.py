from setuptools import setup

setup(
    name="repoclient",
    version="1.1.1",
    packages=["repoclient", "repoclient.models"],
    install_requires=["httpx", "pydantic==2.4.2", "orjson", "pandas==2.1.0", "click==8.1.7"],
)
