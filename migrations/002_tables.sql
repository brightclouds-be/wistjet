-- Tables ("tafels"): typed, timed decks and a time limit per child.
ALTER TABLE deck ADD COLUMN answer_mode TEXT NOT NULL DEFAULT 'self'
    CHECK (answer_mode IN ('self', 'timed'));
ALTER TABLE child ADD COLUMN time_limit_ms INTEGER NOT NULL DEFAULT 3000
    CHECK (time_limit_ms BETWEEN 1000 AND 30000 AND time_limit_ms % 500 = 0);
