import sqlalchemy as sa
from alembic import op


def upgrade():
    op.create_table("bookings", sa.Column("id", sa.Integer))
    op.add_column("booking_items", sa.Column("note", sa.Text))
