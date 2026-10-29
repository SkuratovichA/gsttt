import importlib
import pkg.parent
import pkg.parent as parent_module
from pkg.parent import VALUE
from pkg.parent import VALUE as value, VALUE as other
from . import sibling
from .. import parent as up
from ...far import thing
from pkg import *

importlib.import_module("plugins.csv")
__import__("plugins.json")
importlib.import_module(name)
__all__ = ["value", "other"]
