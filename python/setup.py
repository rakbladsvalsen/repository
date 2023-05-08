from setuptools import setup

setup(
    name="repoclient",
    version="1.0.0",
    packages=["repoclient", "repoclient.models"],
    install_requires=["httpx", "pydantic", "orjson"],
)
