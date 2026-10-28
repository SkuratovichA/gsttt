import os
from os import environ

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "project.settings")
environ.setdefault("PORT", "8000")
os.environ.setdefault(NAME, "x")
