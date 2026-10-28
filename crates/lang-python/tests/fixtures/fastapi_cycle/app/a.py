from fastapi import APIRouter

from app import b

router = APIRouter(prefix="/x")
router.include_router(b.router, prefix="/b")


@router.get("/one")
def one():
    return 1
