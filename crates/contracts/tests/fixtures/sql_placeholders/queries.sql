SELECT * FROM bookings WHERE id = $1;
SELECT * FROM bookings WHERE id = ?;
SELECT * FROM bookings WHERE id = :id;
SELECT * FROM bookings WHERE id = %s;
SELECT * FROM bookings WHERE id = %(id)s;
