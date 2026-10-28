from fastapi import APIRouter

router = APIRouter()


@router.get("/api/users")
def list_users():
    return []
