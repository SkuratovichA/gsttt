use sqlx::PgPool;

pub struct Booking {
    pub id: i64,
}

pub async fn find(pool: &PgPool, id: i64) -> Result<Booking, sqlx::Error> {
    let row =
        sqlx::query_as!(Booking, "SELECT id FROM bookings WHERE id = $1", id)
            .fetch_one(pool)
            .await?;
    Ok(row)
}

pub async fn find_from_file(pool: &PgPool, id: i64) -> Result<Option<i64>, sqlx::Error> {
    let row = sqlx::query_file!("queries/find.sql", id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|row| row.id))
}

pub async fn count(pool: &PgPool, table: &str) -> Result<i64, sqlx::Error> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms")
        .fetch_one(pool)
        .await?;
    let sql = format!("SELECT count(*) FROM {table}");
    let other: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await?;
    Ok(total + other)
}
