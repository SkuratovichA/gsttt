from setuptools import setup

setup(
    name="bookings",
    package_dir={"": "lib"},
    install_requires=[
        "fastapi[standard]>=0.1",  # don't pin the extras
        "sqlalchemy",
    ],
)
