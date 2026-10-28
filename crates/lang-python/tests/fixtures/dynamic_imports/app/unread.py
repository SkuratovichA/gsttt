import importlib

importlib.import_module("...m", package="p.q")
importlib.import_module(".m", package=name)
importlib.import_module(".m", package=f"{base}.q")
importlib.import_module(".m", package="")
