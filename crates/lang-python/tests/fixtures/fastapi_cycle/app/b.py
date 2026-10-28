from fastapi import APIRouter

from app import a

router = APIRouter(prefix="/y")
router.include_router(a.router, prefix="/a")


@router.get("/two")
def two():
    return 2
