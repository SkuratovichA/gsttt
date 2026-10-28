import importlib

importlib.import_module(".local", __package__)
importlib.import_module(".local")
importlib.import_module("p.m", package="other")
