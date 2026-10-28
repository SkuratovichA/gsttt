WITH recent AS (SELECT * FROM bookings) SELECT * FROM recent JOIN booking_items i ON true WHERE id = %s;
