-- ============================================================
-- Olive Young Beauty Retail — PostgreSQL Schema
-- 18 tables: 11 entities + 8 join tables
-- ============================================================

-- --- Regions ---
CREATE TABLE IF NOT EXISTS regions (
  id   TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  type TEXT NOT NULL
);

-- --- Brands ---
CREATE TABLE IF NOT EXISTS brands (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  country      TEXT NOT NULL,
  founded_year INTEGER NOT NULL
);

-- --- Categories (self-referential hierarchy) ---
CREATE TABLE IF NOT EXISTS categories (
  id        TEXT PRIMARY KEY,
  name      TEXT NOT NULL,
  parent_id TEXT REFERENCES categories(id)
);

-- --- Ingredients ---
CREATE TABLE IF NOT EXISTS ingredients (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
  name_inci       TEXT NOT NULL,
  ingredient_type TEXT NOT NULL,
  ewg_grade       INTEGER NOT NULL
);

-- --- Skin Concerns ---
CREATE TABLE IF NOT EXISTS skin_concerns (
  id   TEXT PRIMARY KEY,
  name TEXT NOT NULL
);

-- --- Regulations ---
CREATE TABLE IF NOT EXISTS regulations (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  authority      TEXT NOT NULL,
  status         TEXT NOT NULL,
  effective_date DATE NOT NULL
);

-- --- Products ---
CREATE TABLE IF NOT EXISTS products (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  price       INTEGER NOT NULL,
  size        TEXT NOT NULL,
  launch_date DATE NOT NULL,
  brand_id    TEXT NOT NULL REFERENCES brands(id),
  category_id TEXT NOT NULL REFERENCES categories(id)
);

-- --- Stores ---
CREATE TABLE IF NOT EXISTS stores (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  address    TEXT NOT NULL,
  store_type TEXT NOT NULL,
  open_date  DATE NOT NULL,
  region_id  TEXT NOT NULL REFERENCES regions(id)
);

-- --- Customers ---
CREATE TABLE IF NOT EXISTS customers (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
  age             INTEGER NOT NULL,
  gender          TEXT NOT NULL,
  membership_tier TEXT NOT NULL,
  join_date       DATE NOT NULL,
  region_id       TEXT NOT NULL REFERENCES regions(id),
  referred_by     TEXT REFERENCES customers(id)
);

-- --- Transactions ---
CREATE TABLE IF NOT EXISTS transactions (
  id             TEXT PRIMARY KEY,
  total_amount   INTEGER NOT NULL,
  payment_method TEXT NOT NULL,
  purchased_at   TIMESTAMP NOT NULL,
  customer_id    TEXT NOT NULL REFERENCES customers(id),
  store_id       TEXT NOT NULL REFERENCES stores(id)
);

-- --- Reviews ---
CREATE TABLE IF NOT EXISTS reviews (
  id          TEXT PRIMARY KEY,
  rating      INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 5),
  text        TEXT NOT NULL,
  created_at  TIMESTAMP NOT NULL,
  customer_id TEXT NOT NULL REFERENCES customers(id),
  product_id  TEXT NOT NULL REFERENCES products(id)
);

-- --- Promotions ---
CREATE TABLE IF NOT EXISTS promotions (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  discount_pct INTEGER NOT NULL,
  start_date   DATE NOT NULL,
  end_date     DATE NOT NULL
);

-- --- Join Tables (M:N) ---

CREATE TABLE IF NOT EXISTS product_ingredients (
  product_id        TEXT NOT NULL REFERENCES products(id),
  ingredient_id     TEXT NOT NULL REFERENCES ingredients(id),
  concentration_pct NUMERIC,
  is_key_ingredient BOOLEAN NOT NULL DEFAULT FALSE,
  PRIMARY KEY (product_id, ingredient_id)
);

CREATE TABLE IF NOT EXISTS ingredient_treats (
  ingredient_id   TEXT NOT NULL REFERENCES ingredients(id),
  skin_concern_id TEXT NOT NULL REFERENCES skin_concerns(id),
  efficacy_level  TEXT NOT NULL,
  PRIMARY KEY (ingredient_id, skin_concern_id)
);

CREATE TABLE IF NOT EXISTS ingredient_aggravates (
  ingredient_id   TEXT NOT NULL REFERENCES ingredients(id),
  skin_concern_id TEXT NOT NULL REFERENCES skin_concerns(id),
  severity        TEXT NOT NULL,
  PRIMARY KEY (ingredient_id, skin_concern_id)
);

CREATE TABLE IF NOT EXISTS ingredient_synergies (
  ingredient_a_id TEXT NOT NULL REFERENCES ingredients(id),
  ingredient_b_id TEXT NOT NULL REFERENCES ingredients(id),
  boost_pct       INTEGER NOT NULL,
  mechanism       TEXT NOT NULL,
  PRIMARY KEY (ingredient_a_id, ingredient_b_id)
);

CREATE TABLE IF NOT EXISTS ingredient_conflicts (
  ingredient_a_id TEXT NOT NULL REFERENCES ingredients(id),
  ingredient_b_id TEXT NOT NULL REFERENCES ingredients(id),
  risk_level      TEXT NOT NULL,
  reason          TEXT NOT NULL,
  PRIMARY KEY (ingredient_a_id, ingredient_b_id)
);

CREATE TABLE IF NOT EXISTS ingredient_regulations (
  ingredient_id         TEXT NOT NULL REFERENCES ingredients(id),
  regulation_id         TEXT NOT NULL REFERENCES regulations(id),
  max_concentration_pct NUMERIC NOT NULL,
  PRIMARY KEY (ingredient_id, regulation_id)
);

CREATE TABLE IF NOT EXISTS transaction_items (
  transaction_id TEXT NOT NULL REFERENCES transactions(id),
  product_id     TEXT NOT NULL REFERENCES products(id),
  quantity       INTEGER NOT NULL,
  unit_price     INTEGER NOT NULL,
  PRIMARY KEY (transaction_id, product_id)
);

CREATE TABLE IF NOT EXISTS brand_stores (
  brand_id TEXT NOT NULL REFERENCES brands(id),
  store_id TEXT NOT NULL REFERENCES stores(id),
  PRIMARY KEY (brand_id, store_id)
);

CREATE TABLE IF NOT EXISTS promotion_products (
  promotion_id TEXT NOT NULL REFERENCES promotions(id),
  product_id   TEXT NOT NULL REFERENCES products(id),
  PRIMARY KEY (promotion_id, product_id)
);
