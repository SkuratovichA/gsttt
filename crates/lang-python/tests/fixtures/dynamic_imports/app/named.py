import importlib
from importlib import import_module as load

importlib.import_module(".m", package="p")
load("..m", "p.q")
load(".n", package="p.q")
load(name=".m", package="p")
