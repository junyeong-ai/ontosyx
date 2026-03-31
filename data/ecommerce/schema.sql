-- ============================================================
-- E-Commerce Platform — PostgreSQL Schema
-- 18 tables: 12 entity + 6 join/event tables
-- Designed to exercise all Ontosyx platform capabilities:
--   - Self-referential FK (customers.referred_by)
--   - Hierarchical FK (categories.parent_id)
--   - M:N joins (product_categories, product_suppliers)
--   - Composite PK (inventory)
--   - CHECK constraints + partial indexes
--   - Temporal event streams (shipping_events)
--   - Graph link prediction (product_recommendations)
-- ============================================================

-- === Tier 1: Core Entities ===

CREATE TABLE IF NOT EXISTS customers (
  id           TEXT PRIMARY KEY,
  email        TEXT NOT NULL UNIQUE,
  first_name   TEXT NOT NULL,
  last_name    TEXT NOT NULL,
  tier         TEXT NOT NULL DEFAULT 'standard' CHECK (tier IN ('standard', 'silver', 'gold', 'platinum')),
  phone        TEXT,
  city         TEXT,
  country      TEXT NOT NULL DEFAULT 'US',
  referred_by  TEXT REFERENCES customers(id),
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS brands (
  id       TEXT PRIMARY KEY,
  name     TEXT NOT NULL UNIQUE,
  country  TEXT NOT NULL,
  website  TEXT
);

CREATE TABLE IF NOT EXISTS categories (
  id        TEXT PRIMARY KEY,
  name      TEXT NOT NULL,
  parent_id TEXT REFERENCES categories(id),
  depth     INTEGER NOT NULL DEFAULT 0 CHECK (depth >= 0 AND depth <= 5)
);

CREATE TABLE IF NOT EXISTS products (
  id          TEXT PRIMARY KEY,
  sku         TEXT NOT NULL UNIQUE,
  name        TEXT NOT NULL,
  description TEXT,
  brand_id    TEXT NOT NULL REFERENCES brands(id),
  price       NUMERIC(10,2) NOT NULL CHECK (price >= 0),
  weight_kg   NUMERIC(6,3),
  is_active   BOOLEAN NOT NULL DEFAULT true,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS suppliers (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  country        TEXT NOT NULL,
  lead_time_days INTEGER NOT NULL CHECK (lead_time_days > 0),
  rating         NUMERIC(2,1) CHECK (rating >= 1.0 AND rating <= 5.0)
);

CREATE TABLE IF NOT EXISTS warehouses (
  id       TEXT PRIMARY KEY,
  name     TEXT NOT NULL,
  city     TEXT NOT NULL,
  country  TEXT NOT NULL,
  capacity INTEGER NOT NULL CHECK (capacity > 0)
);

CREATE TABLE IF NOT EXISTS customer_segments (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  description TEXT,
  min_spend   NUMERIC(10,2) NOT NULL DEFAULT 0,
  max_spend   NUMERIC(10,2)
);

-- === Tier 2: Transactions ===

CREATE TABLE IF NOT EXISTS orders (
  id           TEXT PRIMARY KEY,
  customer_id  TEXT NOT NULL REFERENCES customers(id),
  status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'confirmed', 'shipped', 'delivered', 'cancelled', 'returned')),
  total_amount NUMERIC(12,2) NOT NULL CHECK (total_amount >= 0),
  currency     TEXT NOT NULL DEFAULT 'USD',
  ordered_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  shipped_at   TIMESTAMPTZ,
  delivered_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS order_items (
  id           TEXT PRIMARY KEY,
  order_id     TEXT NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  product_id   TEXT NOT NULL REFERENCES products(id),
  quantity     INTEGER NOT NULL CHECK (quantity > 0),
  unit_price   NUMERIC(10,2) NOT NULL CHECK (unit_price >= 0),
  discount_pct NUMERIC(5,2) NOT NULL DEFAULT 0 CHECK (discount_pct >= 0 AND discount_pct <= 100)
);

CREATE TABLE IF NOT EXISTS reviews (
  id          TEXT PRIMARY KEY,
  customer_id TEXT NOT NULL REFERENCES customers(id),
  product_id  TEXT NOT NULL REFERENCES products(id),
  rating      INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
  title       TEXT,
  body        TEXT,
  helpful_count INTEGER NOT NULL DEFAULT 0,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- === Tier 3: Inventory & Supply Chain ===

CREATE TABLE IF NOT EXISTS inventory (
  warehouse_id     TEXT NOT NULL REFERENCES warehouses(id),
  product_id       TEXT NOT NULL REFERENCES products(id),
  quantity         INTEGER NOT NULL DEFAULT 0 CHECK (quantity >= 0),
  last_restocked_at TIMESTAMPTZ,
  PRIMARY KEY (warehouse_id, product_id)
);

-- === Tier 4: Marketing & Campaigns ===

CREATE TABLE IF NOT EXISTS campaigns (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  type       TEXT NOT NULL CHECK (type IN ('discount', 'bundle', 'flash_sale', 'loyalty', 'seasonal')),
  budget     NUMERIC(10,2) NOT NULL CHECK (budget >= 0),
  start_date DATE NOT NULL,
  end_date   DATE NOT NULL,
  CHECK (end_date >= start_date)
);

-- === Tier 5: Join Tables (M:N Relationships) ===

CREATE TABLE IF NOT EXISTS product_categories (
  product_id  TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  category_id TEXT NOT NULL REFERENCES categories(id),
  PRIMARY KEY (product_id, category_id)
);

CREATE TABLE IF NOT EXISTS product_suppliers (
  product_id  TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  supplier_id TEXT NOT NULL REFERENCES suppliers(id),
  cost        NUMERIC(10,2) NOT NULL CHECK (cost >= 0),
  is_primary  BOOLEAN NOT NULL DEFAULT false,
  PRIMARY KEY (product_id, supplier_id)
);

CREATE TABLE IF NOT EXISTS campaign_products (
  campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
  product_id  TEXT NOT NULL REFERENCES products(id),
  discount_pct NUMERIC(5,2) DEFAULT 0,
  PRIMARY KEY (campaign_id, product_id)
);

CREATE TABLE IF NOT EXISTS customer_segment_members (
  customer_id TEXT NOT NULL REFERENCES customers(id),
  segment_id  TEXT NOT NULL REFERENCES customer_segments(id),
  assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (customer_id, segment_id)
);

-- === Tier 6: Event Streams & Graph Links ===

CREATE TABLE IF NOT EXISTS shipping_events (
  id          TEXT PRIMARY KEY,
  order_id    TEXT NOT NULL REFERENCES orders(id),
  event_type  TEXT NOT NULL CHECK (event_type IN ('label_created', 'picked_up', 'in_transit', 'out_for_delivery', 'delivered', 'exception')),
  location    TEXT,
  occurred_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS product_recommendations (
  source_product_id TEXT NOT NULL REFERENCES products(id),
  target_product_id TEXT NOT NULL REFERENCES products(id),
  score             NUMERIC(4,3) NOT NULL CHECK (score >= 0 AND score <= 1),
  algorithm         TEXT NOT NULL DEFAULT 'collaborative_filter',
  PRIMARY KEY (source_product_id, target_product_id),
  CHECK (source_product_id != target_product_id)
);

-- === Indexes for Performance ===

CREATE INDEX IF NOT EXISTS idx_orders_customer      ON orders(customer_id, ordered_at DESC);
CREATE INDEX IF NOT EXISTS idx_order_items_order     ON order_items(order_id);
CREATE INDEX IF NOT EXISTS idx_order_items_product   ON order_items(product_id);
CREATE INDEX IF NOT EXISTS idx_reviews_product       ON reviews(product_id, rating);
CREATE INDEX IF NOT EXISTS idx_reviews_customer      ON reviews(customer_id);
CREATE INDEX IF NOT EXISTS idx_shipping_order        ON shipping_events(order_id, occurred_at);
CREATE INDEX IF NOT EXISTS idx_products_brand        ON products(brand_id);
CREATE INDEX IF NOT EXISTS idx_products_active       ON products(id) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_categories_parent     ON categories(parent_id);
CREATE INDEX IF NOT EXISTS idx_recommendations_score ON product_recommendations(source_product_id, score DESC);
CREATE INDEX IF NOT EXISTS idx_customers_tier        ON customers(tier);
CREATE INDEX IF NOT EXISTS idx_customers_referral    ON customers(referred_by) WHERE referred_by IS NOT NULL;
