-- ============================================================
-- E-Commerce Sample Data — Deterministic Seed
-- All values are fixed (no random(), gen_random_uuid(), NOW())
-- Idempotent: ON CONFLICT DO NOTHING on every INSERT
-- ============================================================

-- === Brands (B001–B020) ===

INSERT INTO brands (id, name, country, website) VALUES
  ('B001', 'TechNova',        'US', 'https://technova.example.com'),
  ('B002', 'AudioWave',       'US', 'https://audiowave.example.com'),
  ('B003', 'PureThread',      'US', 'https://purethread.example.com'),
  ('B004', 'GreenLeaf',       'US', 'https://greenleaf.example.com'),
  ('B005', 'Hauswerk',        'DE', 'https://hauswerk.example.com'),
  ('B006', 'BerlinBrew',      'DE', 'https://berlinbrew.example.com'),
  ('B007', 'SakuraTech',      'JP', 'https://sakuratech.example.com'),
  ('B008', 'ZenCraft',        'JP', 'https://zencraft.example.com'),
  ('B009', 'HanStyle',        'KR', 'https://hanstyle.example.com'),
  ('B010', 'SeoulFit',        'KR', 'https://seoulfit.example.com'),
  ('B011', 'Maison Lumière',  'FR', 'https://maisonlumiere.example.com'),
  ('B012', 'Provençal',       'FR', 'https://provencal.example.com'),
  ('B013', 'Firenze Home',    'IT', 'https://firenzehome.example.com'),
  ('B014', 'Dolce Vita',      'IT', 'https://dolcevita.example.com'),
  ('B015', 'BritCraft',       'UK', 'https://britcraft.example.com'),
  ('B016', 'Thames & Co',     'UK', 'https://thamesandco.example.com'),
  ('B017', 'DragonTech',      'CN', 'https://dragontech.example.com'),
  ('B018', 'SilkRoad',        'CN', 'https://silkroad.example.com'),
  ('B019', 'AlpineGear',      'DE', 'https://alpinegear.example.com'),
  ('B020', 'Pacific Living',  'US', 'https://pacificliving.example.com')
ON CONFLICT DO NOTHING;

-- === Categories (CAT001–CAT040): 8 roots, 16 L1, 16 L2 ===

-- L0 roots
INSERT INTO categories (id, name, parent_id, depth) VALUES
  ('CAT001', 'Electronics',      NULL, 0),
  ('CAT002', 'Clothing',         NULL, 0),
  ('CAT003', 'Home & Kitchen',   NULL, 0),
  ('CAT004', 'Sports & Outdoors',NULL, 0),
  ('CAT005', 'Beauty & Health',  NULL, 0),
  ('CAT006', 'Books & Media',    NULL, 0),
  ('CAT007', 'Food & Beverage',  NULL, 0),
  ('CAT008', 'Toys & Games',     NULL, 0)
ON CONFLICT DO NOTHING;

-- L1 (16 categories, 2 per root)
INSERT INTO categories (id, name, parent_id, depth) VALUES
  ('CAT009', 'Audio',            'CAT001', 1),
  ('CAT010', 'Computers',        'CAT001', 1),
  ('CAT011', 'Men',              'CAT002', 1),
  ('CAT012', 'Women',            'CAT002', 1),
  ('CAT013', 'Kitchen',          'CAT003', 1),
  ('CAT014', 'Living Room',      'CAT003', 1),
  ('CAT015', 'Fitness',          'CAT004', 1),
  ('CAT016', 'Camping',          'CAT004', 1),
  ('CAT017', 'Skincare',         'CAT005', 1),
  ('CAT018', 'Supplements',      'CAT005', 1),
  ('CAT019', 'Fiction',          'CAT006', 1),
  ('CAT020', 'Non-Fiction',      'CAT006', 1),
  ('CAT021', 'Coffee & Tea',     'CAT007', 1),
  ('CAT022', 'Snacks',           'CAT007', 1),
  ('CAT023', 'Board Games',      'CAT008', 1),
  ('CAT024', 'Puzzles',          'CAT008', 1)
ON CONFLICT DO NOTHING;

-- L2 (16 categories, 1 per L1)
INSERT INTO categories (id, name, parent_id, depth) VALUES
  ('CAT025', 'Headphones',       'CAT009', 2),
  ('CAT026', 'Laptops',          'CAT010', 2),
  ('CAT027', 'T-Shirts',         'CAT011', 2),
  ('CAT028', 'Dresses',          'CAT012', 2),
  ('CAT029', 'Cookware',         'CAT013', 2),
  ('CAT030', 'Sofas',            'CAT014', 2),
  ('CAT031', 'Yoga',             'CAT015', 2),
  ('CAT032', 'Tents',            'CAT016', 2),
  ('CAT033', 'Moisturizers',     'CAT017', 2),
  ('CAT034', 'Vitamins',         'CAT018', 2),
  ('CAT035', 'Sci-Fi',           'CAT019', 2),
  ('CAT036', 'Biographies',      'CAT020', 2),
  ('CAT037', 'Single Origin',    'CAT021', 2),
  ('CAT038', 'Granola Bars',     'CAT022', 2),
  ('CAT039', 'Strategy Games',   'CAT023', 2),
  ('CAT040', 'Jigsaw Puzzles',   'CAT024', 2)
ON CONFLICT DO NOTHING;

-- === Suppliers (SUP001–SUP015) ===

INSERT INTO suppliers (id, name, country, lead_time_days, rating) VALUES
  ('SUP001', 'Global Parts Inc',     'US',  5,  4.5),
  ('SUP002', 'FastShip Logistics',   'US',  3,  4.2),
  ('SUP003', 'EuroSupply GmbH',      'DE',  7,  4.7),
  ('SUP004', 'Rhine Distributors',   'DE', 10,  3.9),
  ('SUP005', 'Tokyo Components',     'JP',  8,  4.8),
  ('SUP006', 'Osaka Manufacturing',  'JP', 12,  4.1),
  ('SUP007', 'Seoul Direct',         'KR',  6,  4.3),
  ('SUP008', 'Shenzhen Express',     'CN',  4,  4.0),
  ('SUP009', 'Shanghai Materials',   'CN', 14,  3.8),
  ('SUP010', 'Lyon Textiles',        'FR',  9,  4.6),
  ('SUP011', 'Milan Fabrics',        'IT', 11,  4.4),
  ('SUP012', 'Manchester Supply',    'UK',  6,  4.1),
  ('SUP013', 'Mumbai Trading Co',    'IN',  7,  3.7),
  ('SUP014', 'Sao Paulo Goods',      'BR', 15,  3.5),
  ('SUP015', 'Vancouver Imports',    'CA',  5,  4.3)
ON CONFLICT DO NOTHING;

-- === Warehouses (WH001–WH008) ===

INSERT INTO warehouses (id, name, city, country, capacity) VALUES
  ('WH001', 'East Coast Hub',    'Newark',    'US', 50000),
  ('WH002', 'West Coast Hub',    'Los Angeles','US', 45000),
  ('WH003', 'Central US',        'Dallas',    'US', 30000),
  ('WH004', 'Frankfurt Hub',     'Frankfurt', 'DE', 35000),
  ('WH005', 'London Hub',        'London',    'UK', 25000),
  ('WH006', 'Tokyo Hub',         'Tokyo',     'JP', 40000),
  ('WH007', 'Seoul Hub',         'Seoul',     'KR', 20000),
  ('WH008', 'Shanghai Hub',      'Shanghai',  'CN', 55000)
ON CONFLICT DO NOTHING;

-- === Customer Segments (SEG001–SEG005) ===

INSERT INTO customer_segments (id, name, description, min_spend, max_spend) VALUES
  ('SEG001', 'Bronze',   'New or low-spend customers',     0.00,   99.99),
  ('SEG002', 'Silver',   'Regular customers',            100.00,  499.99),
  ('SEG003', 'Gold',     'Frequent buyers',              500.00, 1999.99),
  ('SEG004', 'Platinum', 'High-value customers',        2000.00, 9999.99),
  ('SEG005', 'VIP',      'Top-tier exclusive members', 10000.00,    NULL)
ON CONFLICT DO NOTHING;

-- === Customers (C001–C100) ===
-- Referral chains: C005->C002->C001, C010->C005->C002->C001, C015->C010->C005->C002
-- Insert referred_by=NULL first, then chains in order

INSERT INTO customers (id, email, first_name, last_name, tier, phone, city, country, referred_by, created_at) VALUES
  ('C001', 'alice.smith@example.com',       'Alice',    'Smith',      'platinum', '555-0001', 'New York',     'US', NULL,  '2025-10-01 08:00:00+00'),
  ('C002', 'bob.johnson@example.com',       'Bob',      'Johnson',    'gold',     '555-0002', 'Los Angeles',  'US', 'C001','2025-10-01 09:00:00+00'),
  ('C003', 'carol.williams@example.com',    'Carol',    'Williams',   'silver',   '555-0003', 'Chicago',      'US', NULL,  '2025-10-01 10:00:00+00'),
  ('C004', 'david.brown@example.com',       'David',    'Brown',      'standard', '555-0004', 'Houston',      'US', NULL,  '2025-10-02 08:00:00+00'),
  ('C005', 'emma.jones@example.com',        'Emma',     'Jones',      'gold',     '555-0005', 'Phoenix',      'US', 'C002','2025-10-02 09:00:00+00'),
  ('C006', 'frank.garcia@example.com',      'Frank',    'Garcia',     'standard', '555-0006', 'Philadelphia', 'US', NULL,  '2025-10-02 10:00:00+00'),
  ('C007', 'grace.miller@example.com',      'Grace',    'Miller',     'silver',   '555-0007', 'San Antonio',  'US', 'C003','2025-10-03 08:00:00+00'),
  ('C008', 'henry.davis@example.com',       'Henry',    'Davis',      'standard', '555-0008', 'San Diego',    'US', NULL,  '2025-10-03 09:00:00+00'),
  ('C009', 'iris.rodriguez@example.com',    'Iris',     'Rodriguez',  'gold',     '555-0009', 'Dallas',       'US', NULL,  '2025-10-03 10:00:00+00'),
  ('C010', 'jack.martinez@example.com',     'Jack',     'Martinez',   'silver',   '555-0010', 'San Jose',     'US', 'C005','2025-10-04 08:00:00+00'),
  ('C011', 'karen.hernandez@example.com',   'Karen',    'Hernandez',  'standard', '555-0011', 'Austin',       'US', NULL,  '2025-10-04 09:00:00+00'),
  ('C012', 'leo.lopez@example.com',         'Leo',      'Lopez',      'silver',   '555-0012', 'Jacksonville', 'US', NULL,  '2025-10-04 10:00:00+00'),
  ('C013', 'mia.gonzalez@example.com',      'Mia',      'Gonzalez',   'gold',     '555-0013', 'Fort Worth',   'US', 'C009','2025-10-05 08:00:00+00'),
  ('C014', 'noah.wilson@example.com',       'Noah',     'Wilson',     'standard', '555-0014', 'Columbus',     'US', NULL,  '2025-10-05 09:00:00+00'),
  ('C015', 'olivia.anderson@example.com',   'Olivia',   'Anderson',   'silver',   '555-0015', 'Charlotte',    'US', 'C010','2025-10-05 10:00:00+00'),
  ('C016', 'peter.thomas@example.com',      'Peter',    'Thomas',     'platinum', '555-0016', 'London',       'UK', NULL,  '2025-10-06 08:00:00+00'),
  ('C017', 'quinn.taylor@example.com',      'Quinn',    'Taylor',     'standard', '555-0017', 'Manchester',   'UK', 'C016','2025-10-06 09:00:00+00'),
  ('C018', 'rachel.moore@example.com',      'Rachel',   'Moore',      'gold',     '555-0018', 'Berlin',       'DE', NULL,  '2025-10-06 10:00:00+00'),
  ('C019', 'sam.jackson@example.com',       'Sam',      'Jackson',    'standard', '555-0019', 'Munich',       'DE', 'C018','2025-10-07 08:00:00+00'),
  ('C020', 'tina.white@example.com',        'Tina',     'White',      'silver',   '555-0020', 'Tokyo',        'JP', NULL,  '2025-10-07 09:00:00+00'),
  ('C021', 'uma.harris@example.com',        'Uma',      'Harris',     'standard', '555-0021', 'Osaka',        'JP', 'C020','2025-10-07 10:00:00+00'),
  ('C022', 'victor.clark@example.com',      'Victor',   'Clark',      'gold',     '555-0022', 'Seoul',        'KR', NULL,  '2025-10-08 08:00:00+00'),
  ('C023', 'wendy.lewis@example.com',       'Wendy',    'Lewis',      'standard', '555-0023', 'Paris',        'FR', NULL,  '2025-10-08 09:00:00+00'),
  ('C024', 'xander.robinson@example.com',   'Xander',   'Robinson',   'silver',   '555-0024', 'Milan',        'IT', NULL,  '2025-10-08 10:00:00+00'),
  ('C025', 'yara.walker@example.com',       'Yara',     'Walker',     'standard', '555-0025', 'Toronto',      'CA', NULL,  '2025-10-09 08:00:00+00'),
  ('C026', 'zach.hall@example.com',         'Zach',     'Hall',       'gold',     '555-0026', 'Sydney',       'AU', NULL,  '2025-10-09 09:00:00+00'),
  ('C027', 'amy.allen@example.com',         'Amy',      'Allen',      'standard', '555-0027', 'Denver',       'US', 'C001','2025-10-09 10:00:00+00'),
  ('C028', 'brian.young@example.com',       'Brian',    'Young',      'silver',   '555-0028', 'Seattle',      'US', NULL,  '2025-10-10 08:00:00+00'),
  ('C029', 'chloe.king@example.com',        'Chloe',    'King',       'standard', '555-0029', 'Boston',       'US', 'C028','2025-10-10 09:00:00+00'),
  ('C030', 'daniel.wright@example.com',     'Daniel',   'Wright',     'gold',     '555-0030', 'Nashville',    'US', NULL,  '2025-10-10 10:00:00+00'),
  ('C031', 'ella.scott@example.com',        'Ella',     'Scott',      'standard', '555-0031', 'Portland',     'US', 'C030','2025-10-11 08:00:00+00'),
  ('C032', 'finn.green@example.com',        'Finn',     'Green',      'silver',   '555-0032', 'Miami',        'US', NULL,  '2025-10-11 09:00:00+00'),
  ('C033', 'gina.adams@example.com',        'Gina',     'Adams',      'standard', '555-0033', 'Atlanta',      'US', NULL,  '2025-10-11 10:00:00+00'),
  ('C034', 'hugo.baker@example.com',        'Hugo',     'Baker',      'gold',     '555-0034', 'Detroit',      'US', 'C032','2025-10-12 08:00:00+00'),
  ('C035', 'ivy.nelson@example.com',        'Ivy',      'Nelson',     'standard', '555-0035', 'Memphis',      'US', NULL,  '2025-10-12 09:00:00+00'),
  ('C036', 'jake.carter@example.com',       'Jake',     'Carter',     'silver',   '555-0036', 'Baltimore',    'US', NULL,  '2025-10-12 10:00:00+00'),
  ('C037', 'kate.mitchell@example.com',     'Kate',     'Mitchell',   'standard', '555-0037', 'Milwaukee',    'US', 'C036','2025-10-13 08:00:00+00'),
  ('C038', 'liam.perez@example.com',        'Liam',     'Perez',      'gold',     '555-0038', 'Las Vegas',    'US', NULL,  '2025-10-13 09:00:00+00'),
  ('C039', 'maya.roberts@example.com',      'Maya',     'Roberts',    'standard', '555-0039', 'Louisville',   'US', 'C038','2025-10-13 10:00:00+00'),
  ('C040', 'nathan.turner@example.com',     'Nathan',   'Turner',     'platinum', '555-0040', 'Oklahoma City','US', NULL,  '2025-10-14 08:00:00+00'),
  ('C041', 'olive.phillips@example.com',    'Olive',    'Phillips',   'standard', '555-0041', 'Richmond',     'US', 'C040','2025-10-14 09:00:00+00'),
  ('C042', 'paul.campbell@example.com',     'Paul',     'Campbell',   'silver',   '555-0042', 'Salt Lake City','US',NULL,  '2025-10-14 10:00:00+00'),
  ('C043', 'rose.parker@example.com',       'Rose',     'Parker',     'standard', '555-0043', 'Hartford',     'US', NULL,  '2025-10-15 08:00:00+00'),
  ('C044', 'seth.evans@example.com',        'Seth',     'Evans',      'gold',     '555-0044', 'Raleigh',      'US', 'C040','2025-10-15 09:00:00+00'),
  ('C045', 'tara.edwards@example.com',      'Tara',     'Edwards',    'standard', '555-0045', 'Birmingham',   'UK', NULL,  '2025-10-15 10:00:00+00'),
  ('C046', 'uriel.collins@example.com',     'Uriel',    'Collins',    'silver',   '555-0046', 'Edinburgh',    'UK', 'C016','2025-10-16 08:00:00+00'),
  ('C047', 'vera.stewart@example.com',      'Vera',     'Stewart',    'standard', '555-0047', 'Hamburg',      'DE', NULL,  '2025-10-16 09:00:00+00'),
  ('C048', 'wade.sanchez@example.com',      'Wade',     'Sanchez',    'gold',     '555-0048', 'Cologne',      'DE', 'C018','2025-10-16 10:00:00+00'),
  ('C049', 'xena.morris@example.com',       'Xena',     'Morris',     'standard', '555-0049', 'Yokohama',     'JP', NULL,  '2025-10-17 08:00:00+00'),
  ('C050', 'yuri.rogers@example.com',       'Yuri',     'Rogers',     'silver',   '555-0050', 'Busan',        'KR', 'C022','2025-10-17 09:00:00+00'),
  ('C051', 'adam.cook@example.com',         'Adam',     'Cook',       'standard', '555-0051', 'New York',     'US', NULL,  '2025-10-17 10:00:00+00'),
  ('C052', 'beth.morgan@example.com',       'Beth',     'Morgan',     'silver',   '555-0052', 'Chicago',      'US', NULL,  '2025-10-18 08:00:00+00'),
  ('C053', 'carl.bell@example.com',         'Carl',     'Bell',       'standard', '555-0053', 'Houston',      'US', 'C052','2025-10-18 09:00:00+00'),
  ('C054', 'diana.murphy@example.com',      'Diana',    'Murphy',     'gold',     '555-0054', 'Phoenix',      'US', NULL,  '2025-10-18 10:00:00+00'),
  ('C055', 'eric.bailey@example.com',       'Eric',     'Bailey',     'standard', '555-0055', 'San Antonio',  'US', 'C054','2025-10-19 08:00:00+00'),
  ('C056', 'fiona.rivera@example.com',      'Fiona',    'Rivera',     'silver',   '555-0056', 'Dallas',       'US', NULL,  '2025-10-19 09:00:00+00'),
  ('C057', 'george.cooper@example.com',     'George',   'Cooper',     'standard', '555-0057', 'Austin',       'US', NULL,  '2025-10-19 10:00:00+00'),
  ('C058', 'helen.cox@example.com',         'Helen',    'Cox',        'gold',     '555-0058', 'San Jose',     'US', 'C054','2025-10-20 08:00:00+00'),
  ('C059', 'ivan.howard@example.com',       'Ivan',     'Howard',     'standard', '555-0059', 'Jacksonville', 'US', NULL,  '2025-10-20 09:00:00+00'),
  ('C060', 'jill.ward@example.com',         'Jill',     'Ward',       'silver',   '555-0060', 'Columbus',     'US', NULL,  '2025-10-20 10:00:00+00'),
  ('C061', 'kurt.torres@example.com',       'Kurt',     'Torres',     'standard', '555-0061', 'Charlotte',    'US', 'C060','2025-10-21 08:00:00+00'),
  ('C062', 'lisa.peterson@example.com',     'Lisa',     'Peterson',   'gold',     '555-0062', 'Fort Worth',   'US', NULL,  '2025-10-21 09:00:00+00'),
  ('C063', 'mark.gray@example.com',         'Mark',     'Gray',       'standard', '555-0063', 'Denver',       'US', NULL,  '2025-10-21 10:00:00+00'),
  ('C064', 'nina.ramirez@example.com',      'Nina',     'Ramirez',    'silver',   '555-0064', 'Seattle',      'US', 'C062','2025-10-22 08:00:00+00'),
  ('C065', 'oscar.james@example.com',       'Oscar',    'James',      'standard', '555-0065', 'Boston',       'US', NULL,  '2025-10-22 09:00:00+00'),
  ('C066', 'penny.watson@example.com',      'Penny',    'Watson',     'gold',     '555-0066', 'Nashville',    'US', NULL,  '2025-10-22 10:00:00+00'),
  ('C067', 'rex.brooks@example.com',        'Rex',      'Brooks',     'standard', '555-0067', 'Portland',     'US', 'C066','2025-10-23 08:00:00+00'),
  ('C068', 'sara.price@example.com',        'Sara',     'Price',      'silver',   '555-0068', 'Miami',        'US', NULL,  '2025-10-23 09:00:00+00'),
  ('C069', 'tom.bennett@example.com',       'Tom',      'Bennett',    'standard', '555-0069', 'Atlanta',      'US', NULL,  '2025-10-23 10:00:00+00'),
  ('C070', 'ursula.wood@example.com',       'Ursula',   'Wood',       'gold',     '555-0070', 'Detroit',      'US', 'C066','2025-10-24 08:00:00+00'),
  ('C071', 'vince.barnes@example.com',      'Vince',    'Barnes',     'standard', '555-0071', 'Memphis',      'US', NULL,  '2025-10-24 09:00:00+00'),
  ('C072', 'wilma.ross@example.com',        'Wilma',    'Ross',       'silver',   '555-0072', 'Baltimore',    'US', NULL,  '2025-10-24 10:00:00+00'),
  ('C073', 'xavi.henderson@example.com',    'Xavi',     'Henderson',  'standard', '555-0073', 'Milwaukee',    'US', 'C072','2025-10-25 08:00:00+00'),
  ('C074', 'yvonne.coleman@example.com',    'Yvonne',   'Coleman',    'gold',     '555-0074', 'Las Vegas',    'US', NULL,  '2025-10-25 09:00:00+00'),
  ('C075', 'zane.jenkins@example.com',      'Zane',     'Jenkins',    'standard', '555-0075', 'Louisville',   'US', 'C074','2025-10-25 10:00:00+00'),
  ('C076', 'anna.perry@example.com',        'Anna',     'Perry',      'platinum', '555-0076', 'London',       'UK', NULL,  '2025-10-26 08:00:00+00'),
  ('C077', 'ben.powell@example.com',        'Ben',      'Powell',     'standard', '555-0077', 'Leeds',        'UK', 'C076','2025-10-26 09:00:00+00'),
  ('C078', 'clara.long@example.com',        'Clara',    'Long',       'silver',   '555-0078', 'Bristol',      'UK', NULL,  '2025-10-26 10:00:00+00'),
  ('C079', 'derek.hughes@example.com',      'Derek',    'Hughes',     'standard', '555-0079', 'Frankfurt',    'DE', NULL,  '2025-10-27 08:00:00+00'),
  ('C080', 'eva.flores@example.com',        'Eva',      'Flores',     'gold',     '555-0080', 'Stuttgart',    'DE', 'C018','2025-10-27 09:00:00+00'),
  ('C081', 'fred.russell@example.com',      'Fred',     'Russell',    'standard', '555-0081', 'Kyoto',        'JP', NULL,  '2025-10-27 10:00:00+00'),
  ('C082', 'gail.diaz@example.com',         'Gail',     'Diaz',       'silver',   '555-0082', 'Sapporo',      'JP', 'C020','2025-10-28 08:00:00+00'),
  ('C083', 'hank.sanders@example.com',      'Hank',     'Sanders',    'standard', '555-0083', 'Incheon',      'KR', NULL,  '2025-10-28 09:00:00+00'),
  ('C084', 'ida.foster@example.com',        'Ida',      'Foster',     'gold',     '555-0084', 'Lyon',         'FR', NULL,  '2025-10-28 10:00:00+00'),
  ('C085', 'joel.gonzales@example.com',     'Joel',     'Gonzales',   'standard', '555-0085', 'Rome',         'IT', 'C084','2025-10-29 08:00:00+00'),
  ('C086', 'kim.butler@example.com',        'Kim',      'Butler',     'silver',   '555-0086', 'New York',     'US', NULL,  '2025-10-29 09:00:00+00'),
  ('C087', 'lance.simmons@example.com',     'Lance',    'Simmons',    'standard', '555-0087', 'Los Angeles',  'US', 'C086','2025-10-29 10:00:00+00'),
  ('C088', 'megan.foster@example.com',      'Megan',    'Foster',     'gold',     '555-0088', 'Chicago',      'US', NULL,  '2025-10-30 08:00:00+00'),
  ('C089', 'nate.bryant@example.com',       'Nate',     'Bryant',     'standard', '555-0089', 'Houston',      'US', 'C088','2025-10-30 09:00:00+00'),
  ('C090', 'opal.alexander@example.com',    'Opal',     'Alexander',  'silver',   '555-0090', 'Phoenix',      'US', NULL,  '2025-10-30 10:00:00+00'),
  ('C091', 'phil.russell2@example.com',     'Phil',     'Russell',    'standard', '555-0091', 'San Diego',    'US', NULL,  '2025-10-31 08:00:00+00'),
  ('C092', 'quinn.reed@example.com',        'Quinn',    'Reed',       'gold',     '555-0092', 'Dallas',       'US', 'C090','2025-10-31 09:00:00+00'),
  ('C093', 'ruth.kelly@example.com',        'Ruth',     'Kelly',      'standard', '555-0093', 'San Jose',     'US', NULL,  '2025-10-31 10:00:00+00'),
  ('C094', 'steve.price@example.com',       'Steve',    'Price',      'silver',   '555-0094', 'Austin',       'US', NULL,  '2025-11-01 08:00:00+00'),
  ('C095', 'tess.howard@example.com',       'Tess',     'Howard',     'standard', '555-0095', 'Jacksonville', 'US', 'C094','2025-11-01 09:00:00+00'),
  ('C096', 'vance.ward@example.com',        'Vance',    'Ward',       'gold',     '555-0096', 'Fort Worth',   'US', NULL,  '2025-11-01 10:00:00+00'),
  ('C097', 'wren.torres@example.com',       'Wren',     'Torres',     'standard', '555-0097', 'Columbus',     'US', 'C096','2025-11-02 08:00:00+00'),
  ('C098', 'xyla.peterson@example.com',     'Xyla',     'Peterson',   'silver',   '555-0098', 'Charlotte',    'US', NULL,  '2025-11-02 09:00:00+00'),
  ('C099', 'yosef.james@example.com',       'Yosef',    'James',      'standard', '555-0099', 'Denver',       'US', 'C098','2025-11-02 10:00:00+00'),
  ('C100', 'zelda.brooks@example.com',      'Zelda',    'Brooks',     'platinum', '555-0100', 'Seattle',      'US', NULL,  '2025-11-03 08:00:00+00')
ON CONFLICT DO NOTHING;

-- === Products (P001–P200) ===

INSERT INTO products (id, sku, name, description, brand_id, price, weight_kg, is_active, created_at) VALUES
  -- Electronics / Audio (P001-P020)
  ('P001', 'SKU-0001', 'Wireless Earbuds Pro',       'Premium true wireless earbuds with ANC', 'B002', 149.99, 0.045, true,  '2025-10-01 06:00:00+00'),
  ('P002', 'SKU-0002', 'Over-Ear Studio Headphones', 'Professional studio-grade headphones',   'B002', 249.99, 0.320, true,  '2025-10-01 06:00:00+00'),
  ('P003', 'SKU-0003', 'Bluetooth Speaker Mini',     'Portable waterproof speaker',            'B002', 59.99,  0.280, true,  '2025-10-01 06:00:00+00'),
  ('P004', 'SKU-0004', 'Noise Cancelling Buds',      'Active noise cancellation earbuds',      'B007', 129.99, 0.040, true,  '2025-10-01 06:00:00+00'),
  ('P005', 'SKU-0005', 'DJ Headphones',              'Heavy bass DJ headphones',               'B007', 199.99, 0.350, true,  '2025-10-01 06:00:00+00'),
  ('P006', 'SKU-0006', 'Soundbar 2.1',               'Home theater soundbar with subwoofer',   'B001', 329.99, 4.500, true,  '2025-10-01 06:00:00+00'),
  ('P007', 'SKU-0007', 'Podcast Microphone',         'USB condenser microphone',               'B001', 89.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P008', 'SKU-0008', 'DAC Amplifier',              'Hi-res desktop DAC/amp',                 'B007', 179.99, 0.400, true,  '2025-10-01 06:00:00+00'),
  ('P009', 'SKU-0009', 'Turntable Classic',          'Belt-drive vinyl turntable',             'B015', 279.99, 5.200, true,  '2025-10-01 06:00:00+00'),
  ('P010', 'SKU-0010', 'In-Ear Monitors',            'Triple driver IEM for musicians',        'B002', 199.99, 0.030, true,  '2025-10-01 06:00:00+00'),
  ('P011', 'SKU-0011', 'Wireless Earbuds Lite',      'Budget wireless earbuds',                'B017', 39.99,  0.035, true,  '2025-10-01 06:00:00+00'),
  ('P012', 'SKU-0012', 'Smart Speaker',              'Voice assistant smart speaker',          'B001', 99.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P013', 'SKU-0013', 'Gaming Headset',             'RGB gaming headset with mic',            'B017', 79.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P014', 'SKU-0014', 'Car Bluetooth Adapter',      'AUX to Bluetooth car adapter',           'B001', 24.99,  0.050, true,  '2025-10-01 06:00:00+00'),
  ('P015', 'SKU-0015', 'Record Player Stylus',       'Replacement diamond stylus',             'B015', 14.99,  0.005, true,  '2025-10-01 06:00:00+00'),
  ('P016', 'SKU-0016', 'Karaoke Mic Set',            'Dual wireless karaoke set',              'B009', 69.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P017', 'SKU-0017', 'Bone Conduction Phones',     'Open-ear bone conduction headphones',    'B010', 109.99, 0.036, true,  '2025-10-01 06:00:00+00'),
  ('P018', 'SKU-0018', 'Studio Monitor Pair',        'Active 5-inch studio monitors',          'B005', 399.99, 8.000, true,  '2025-10-01 06:00:00+00'),
  ('P019', 'SKU-0019', 'Audio Interface 2i2',        'USB audio interface 2-in 2-out',         'B015', 149.99, 0.360, true,  '2025-10-01 06:00:00+00'),
  ('P020', 'SKU-0020', 'Headphone Stand',            'Walnut wood headphone stand',            'B008', 34.99,  0.450, true,  '2025-10-01 06:00:00+00'),

  -- Electronics / Computers (P021-P040)
  ('P021', 'SKU-0021', 'Ultrabook 14 Pro',           '14-inch ultralight laptop',              'B001', 499.99, 1.200, true,  '2025-10-01 06:00:00+00'),
  ('P022', 'SKU-0022', 'Mechanical Keyboard',        'Cherry MX Brown mechanical keyboard',    'B007', 129.99, 0.900, true,  '2025-10-01 06:00:00+00'),
  ('P023', 'SKU-0023', 'Ergonomic Mouse',            'Vertical ergonomic wireless mouse',      'B005', 49.99,  0.120, true,  '2025-10-01 06:00:00+00'),
  ('P024', 'SKU-0024', 'USB-C Hub 7-in-1',           'Multiport USB-C docking hub',            'B001', 44.99,  0.150, true,  '2025-10-01 06:00:00+00'),
  ('P025', 'SKU-0025', '27-inch 4K Monitor',         'IPS 4K USB-C monitor',                   'B017', 349.99, 6.500, true,  '2025-10-01 06:00:00+00'),
  ('P026', 'SKU-0026', 'Laptop Stand',               'Adjustable aluminum laptop stand',       'B008', 39.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P027', 'SKU-0027', 'Webcam HD 1080p',            'Full HD webcam with auto-focus',         'B001', 59.99,  0.150, true,  '2025-10-01 06:00:00+00'),
  ('P028', 'SKU-0028', 'External SSD 1TB',           'Portable NVMe SSD 1TB',                  'B017', 89.99,  0.060, true,  '2025-10-01 06:00:00+00'),
  ('P029', 'SKU-0029', 'Wireless Keyboard Slim',     'Low-profile wireless keyboard',          'B005', 69.99,  0.450, true,  '2025-10-01 06:00:00+00'),
  ('P030', 'SKU-0030', 'Mouse Pad XL',               'Extended desk mat mouse pad',            'B008', 19.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P031', 'SKU-0031', 'Cable Management Kit',       '25-piece cable organizer set',           'B020', 15.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P032', 'SKU-0032', 'Monitor Light Bar',          'Screen-mounted LED light bar',           'B008', 49.99,  0.380, true,  '2025-10-01 06:00:00+00'),
  ('P033', 'SKU-0033', 'Portable Charger 20K',       '20000mAh power bank',                    'B017', 34.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P034', 'SKU-0034', 'Graphics Tablet',            'Digital drawing tablet with pen',         'B007', 199.99, 0.700, true,  '2025-10-01 06:00:00+00'),
  ('P035', 'SKU-0035', 'Desk Lamp LED',              'Adjustable LED desk lamp',               'B005', 29.99,  0.650, true,  '2025-10-01 06:00:00+00'),
  ('P036', 'SKU-0036', 'Privacy Screen 14"',         'Laptop privacy filter 14 inch',          'B001', 24.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P037', 'SKU-0037', 'Wi-Fi Mesh Router',          'Tri-band mesh router 3-pack',            'B001', 279.99, 1.200, true,  '2025-10-01 06:00:00+00'),
  ('P038', 'SKU-0038', 'Smart Plug 4-Pack',          'Wi-Fi smart plugs with timer',           'B001', 29.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P039', 'SKU-0039', 'Wireless Charger Pad',       'Qi fast wireless charger',               'B009', 19.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P040', 'SKU-0040', 'Laptop Sleeve 14"',          'Neoprene laptop sleeve',                 'B003', 22.99,  0.150, true,  '2025-10-01 06:00:00+00'),

  -- Clothing / Men (P041-P070)
  ('P041', 'SKU-0041', 'Organic Cotton T-Shirt',     '100% organic cotton crew neck',          'B003', 29.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P042', 'SKU-0042', 'Slim Fit Chinos',            'Stretch slim-fit chino pants',           'B003', 49.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P043', 'SKU-0043', 'Merino Wool Sweater',        'Lightweight merino crew neck',           'B015', 89.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P044', 'SKU-0044', 'Oxford Shirt Classic',       'Button-down Oxford shirt',               'B016', 59.99,  0.250, true,  '2025-10-01 06:00:00+00'),
  ('P045', 'SKU-0045', 'Denim Jacket',               'Classic wash denim trucker jacket',      'B003', 79.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P046', 'SKU-0046', 'Performance Polo',           'Moisture-wicking golf polo',             'B010', 44.99,  0.220, true,  '2025-10-01 06:00:00+00'),
  ('P047', 'SKU-0047', 'Linen Shorts',               'Relaxed-fit linen blend shorts',         'B011', 39.99,  0.250, true,  '2025-10-01 06:00:00+00'),
  ('P048', 'SKU-0048', 'Flannel Shirt',              'Heavyweight plaid flannel',              'B020', 54.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P049', 'SKU-0049', 'Leather Belt',               'Full-grain leather dress belt',          'B014', 34.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P050', 'SKU-0050', 'Athletic Joggers',           'Tapered athletic jogger pants',          'B010', 44.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P051', 'SKU-0051', 'Cotton Boxer Briefs 3-Pack', 'Premium cotton boxer briefs',            'B003', 24.99,  0.150, true,  '2025-10-01 06:00:00+00'),
  ('P052', 'SKU-0052', 'Crew Socks 6-Pack',          'Athletic crew socks bundle',             'B010', 14.99,  0.180, true,  '2025-10-01 06:00:00+00'),
  ('P053', 'SKU-0053', 'Puffer Vest',                'Lightweight down puffer vest',           'B019', 99.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P054', 'SKU-0054', 'Rain Jacket',                'Waterproof packable rain jacket',        'B019', 79.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P055', 'SKU-0055', 'Wool Beanie',                'Ribbed knit wool beanie',                'B015', 19.99,  0.080, true,  '2025-10-01 06:00:00+00'),
  ('P056', 'SKU-0056', 'Canvas Sneakers',            'Classic low-top canvas sneakers',        'B009', 49.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P057', 'SKU-0057', 'Swim Trunks',                'Quick-dry swim trunks',                  'B020', 29.99,  0.180, true,  '2025-10-01 06:00:00+00'),
  ('P058', 'SKU-0058', 'Dress Socks 4-Pack',         'Patterned dress socks bundle',           'B016', 19.99,  0.120, true,  '2025-10-01 06:00:00+00'),
  ('P059', 'SKU-0059', 'Henley Long Sleeve',         'Cotton henley long sleeve tee',          'B003', 34.99,  0.250, true,  '2025-10-01 06:00:00+00'),
  ('P060', 'SKU-0060', 'Cargo Shorts',               'Relaxed fit cargo shorts',               'B020', 39.99,  0.350, true,  '2025-10-01 06:00:00+00'),

  -- Clothing / Women (P061-P090)
  ('P061', 'SKU-0061', 'Wrap Dress',                 'Floral print wrap dress',                'B011', 69.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P062', 'SKU-0062', 'High-Rise Skinny Jeans',     'Stretch skinny jeans',                   'B009', 59.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P063', 'SKU-0063', 'Cashmere Scarf',             'Lightweight cashmere scarf',             'B011', 89.99,  0.150, true,  '2025-10-01 06:00:00+00'),
  ('P064', 'SKU-0064', 'Silk Blouse',                'Button-front silk blouse',               'B012', 79.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P065', 'SKU-0065', 'Yoga Leggings',              'High-waist compression leggings',        'B010', 54.99,  0.250, true,  '2025-10-01 06:00:00+00'),
  ('P066', 'SKU-0066', 'Trench Coat',                'Classic double-breasted trench coat',    'B016', 149.99, 1.200, true,  '2025-10-01 06:00:00+00'),
  ('P067', 'SKU-0067', 'Midi Skirt',                 'Pleated midi skirt',                     'B011', 49.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P068', 'SKU-0068', 'Lace Camisole',              'Delicate lace trim camisole',            'B012', 29.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P069', 'SKU-0069', 'Wool Coat',                  'Double-faced wool long coat',            'B013', 199.99, 1.800, true,  '2025-10-01 06:00:00+00'),
  ('P070', 'SKU-0070', 'Ballet Flats',               'Leather ballet flat shoes',              'B014', 64.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P071', 'SKU-0071', 'Cotton Sundress',            'A-line cotton sundress',                 'B003', 44.99,  0.250, true,  '2025-10-01 06:00:00+00'),
  ('P072', 'SKU-0072', 'Sports Bra',                 'Medium-support sports bra',              'B010', 34.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P073', 'SKU-0073', 'Ankle Boots',                'Suede ankle boots with heel',            'B013', 119.99, 0.800, true,  '2025-10-01 06:00:00+00'),
  ('P074', 'SKU-0074', 'Crossbody Bag',              'Leather crossbody purse',                'B014', 89.99,  0.450, true,  '2025-10-01 06:00:00+00'),
  ('P075', 'SKU-0075', 'Wide Leg Pants',             'Flowing wide-leg trousers',              'B011', 59.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P076', 'SKU-0076', 'Knit Cardigan',              'Oversized button-front cardigan',        'B015', 69.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P077', 'SKU-0077', 'Denim Shorts',               'High-rise cutoff denim shorts',          'B009', 34.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P078', 'SKU-0078', 'Tote Bag Canvas',            'Large canvas tote bag',                  'B003', 24.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P079', 'SKU-0079', 'Running Shoes',              'Lightweight running sneakers',           'B010', 99.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P080', 'SKU-0080', 'Pearl Earrings',             'Freshwater pearl drop earrings',         'B009', 44.99,  0.020, true,  '2025-10-01 06:00:00+00'),

  -- Home & Kitchen (P081-P120)
  ('P081', 'SKU-0081', 'Ceramic Coffee Mug',         'Handcrafted 12oz ceramic mug',           'B008', 14.99,  0.350, true,  '2025-10-01 06:00:00+00'),
  ('P082', 'SKU-0082', 'Cast Iron Skillet 10"',      'Pre-seasoned cast iron skillet',         'B005', 39.99,  3.200, true,  '2025-10-01 06:00:00+00'),
  ('P083', 'SKU-0083', 'Chef Knife 8"',              'German steel chef knife',                'B005', 79.99,  0.250, true,  '2025-10-01 06:00:00+00'),
  ('P084', 'SKU-0084', 'Cutting Board Bamboo',       'Large bamboo cutting board',             'B008', 24.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P085', 'SKU-0085', 'French Press',               'Stainless steel French press',           'B006', 34.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P086', 'SKU-0086', 'Pour Over Dripper',          'Ceramic pour-over coffee dripper',       'B008', 19.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P087', 'SKU-0087', 'Electric Kettle',            '1.7L gooseneck electric kettle',         'B005', 59.99,  0.900, true,  '2025-10-01 06:00:00+00'),
  ('P088', 'SKU-0088', 'Mixing Bowl Set',            '5-piece stainless steel mixing bowls',   'B013', 29.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P089', 'SKU-0089', 'Baking Sheet Set',           'Non-stick baking sheets 2-pack',         'B013', 19.99,  1.000, true,  '2025-10-01 06:00:00+00'),
  ('P090', 'SKU-0090', 'Spice Rack Organizer',       '24-jar revolving spice rack',            'B020', 34.99,  2.000, true,  '2025-10-01 06:00:00+00'),
  ('P091', 'SKU-0091', 'Throw Blanket',              'Soft knit throw blanket',                'B015', 44.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P092', 'SKU-0092', 'Scented Candle Set',         '3-piece soy wax candle set',             'B011', 29.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P093', 'SKU-0093', 'Plant Pot Ceramic',          'Modern ceramic planter 6 inch',          'B008', 18.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P094', 'SKU-0094', 'Picture Frame Set',          'Gallery wall frame set 5-pack',          'B020', 39.99,  2.000, true,  '2025-10-01 06:00:00+00'),
  ('P095', 'SKU-0095', 'Bath Towel Set',             'Egyptian cotton towels 4-pack',          'B016', 49.99,  1.600, true,  '2025-10-01 06:00:00+00'),
  ('P096', 'SKU-0096', 'Doormat Welcome',            'Coir welcome doormat',                   'B020', 19.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P097', 'SKU-0097', 'Coaster Set Marble',         'Marble coasters 6-pack',                 'B013', 24.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P098', 'SKU-0098', 'Storage Baskets',            'Woven storage baskets 3-pack',           'B018', 34.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P099', 'SKU-0099', 'Bed Sheet Set Queen',        'Microfiber bed sheets queen',            'B003', 39.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P100', 'SKU-0100', 'Pillow Memory Foam',         'Cooling memory foam pillow',             'B020', 49.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P101', 'SKU-0101', 'Dutch Oven 5Qt',             'Enameled cast iron Dutch oven',          'B013', 89.99,  5.500, true,  '2025-10-01 06:00:00+00'),
  ('P102', 'SKU-0102', 'Utensil Set Silicone',       '10-piece silicone cooking utensils',     'B020', 24.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P103', 'SKU-0103', 'Wine Glasses Set',           'Crystal wine glasses 4-pack',            'B013', 34.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P104', 'SKU-0104', 'Kitchen Timer Digital',      'Magnetic digital kitchen timer',         'B005', 9.99,   0.060, true,  '2025-10-01 06:00:00+00'),
  ('P105', 'SKU-0105', 'Apron Cotton',               'Chef apron with pockets',                'B003', 19.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P106', 'SKU-0106', 'Salt & Pepper Mill Set',     'Adjustable grinder mill set',            'B005', 29.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P107', 'SKU-0107', 'Tea Kettle Stovetop',        'Stainless steel whistling kettle',       'B006', 29.99,  0.700, true,  '2025-10-01 06:00:00+00'),
  ('P108', 'SKU-0108', 'Laundry Basket',             'Collapsible fabric laundry hamper',      'B018', 22.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P109', 'SKU-0109', 'Wall Clock Modern',          'Minimalist wall clock 12 inch',          'B008', 34.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P110', 'SKU-0110', 'Vacuum Thermos',             'Double-wall vacuum flask 750ml',         'B007', 29.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P111', 'SKU-0111', 'Knife Sharpener',            'Two-stage knife sharpener',              'B005', 14.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P112', 'SKU-0112', 'Ice Cube Tray Silicone',     'Large cube silicone tray 2-pack',        'B020', 9.99,   0.150, true,  '2025-10-01 06:00:00+00'),
  ('P113', 'SKU-0113', 'Oven Mitts Pair',            'Heat-resistant silicone oven mitts',     'B020', 14.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P114', 'SKU-0114', 'Dish Drying Rack',           'Stainless steel dish rack',              'B005', 29.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P115', 'SKU-0115', 'Reusable Food Wrap',         'Beeswax food wrap 3-pack',               'B004', 14.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P116', 'SKU-0116', 'Glass Food Containers',      'Meal prep containers 5-pack',            'B020', 24.99,  1.800, true,  '2025-10-01 06:00:00+00'),
  ('P117', 'SKU-0117', 'Wooden Serving Board',       'Acacia wood serving board',              'B008', 29.99,  1.000, true,  '2025-10-01 06:00:00+00'),
  ('P118', 'SKU-0118', 'Cotton Napkins 8-Pack',      'Linen-cotton blend napkins',             'B003', 19.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P119', 'SKU-0119', 'Espresso Machine',           'Manual lever espresso maker',            'B014', 249.99, 4.500, true,  '2025-10-01 06:00:00+00'),
  ('P120', 'SKU-0120', 'Coffee Grinder Burr',        'Conical burr coffee grinder',            'B006', 69.99,  1.200, true,  '2025-10-01 06:00:00+00'),

  -- Sports & Outdoors (P121-P150)
  ('P121', 'SKU-0121', 'Yoga Mat Premium',           '6mm non-slip yoga mat',                  'B010', 39.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P122', 'SKU-0122', 'Resistance Bands Set',       '5-level resistance band set',            'B010', 19.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P123', 'SKU-0123', 'Dumbbell Set 25lb',          'Adjustable dumbbell pair 25lb each',     'B019', 149.99, 22.700,true,  '2025-10-01 06:00:00+00'),
  ('P124', 'SKU-0124', 'Jump Rope Speed',            'Ball bearing speed jump rope',           'B010', 14.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P125', 'SKU-0125', 'Foam Roller',                'High-density foam roller 18 inch',       'B010', 24.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P126', 'SKU-0126', 'Hiking Backpack 40L',        'Waterproof hiking backpack',             'B019', 89.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P127', 'SKU-0127', 'Camping Tent 3-Person',      'Lightweight 3-person dome tent',         'B019', 179.99, 2.800, true,  '2025-10-01 06:00:00+00'),
  ('P128', 'SKU-0128', 'Sleeping Bag 30°F',          'Mummy sleeping bag rated to 30°F',       'B019', 79.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P129', 'SKU-0129', 'Headlamp LED',               '300-lumen rechargeable headlamp',        'B019', 24.99,  0.080, true,  '2025-10-01 06:00:00+00'),
  ('P130', 'SKU-0130', 'Water Bottle Insulated',     '32oz insulated water bottle',            'B004', 29.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P131', 'SKU-0131', 'Trekking Poles Pair',        'Carbon fiber trekking poles',            'B019', 59.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P132', 'SKU-0132', 'Camping Hammock',            'Nylon hammock with straps',              'B004', 34.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P133', 'SKU-0133', 'Fitness Tracker Band',       'Basic fitness activity tracker',         'B017', 49.99,  0.030, true,  '2025-10-01 06:00:00+00'),
  ('P134', 'SKU-0134', 'Yoga Block Set',             'Cork yoga blocks 2-pack',                'B010', 19.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P135', 'SKU-0135', 'Exercise Ball 65cm',         'Anti-burst stability ball',              'B010', 24.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P136', 'SKU-0136', 'Cycling Gloves',             'Padded cycling gloves',                  'B019', 19.99,  0.080, true,  '2025-10-01 06:00:00+00'),
  ('P137', 'SKU-0137', 'Sports Sunglasses',          'Polarized sport sunglasses',             'B009', 44.99,  0.030, true,  '2025-10-01 06:00:00+00'),
  ('P138', 'SKU-0138', 'Camping Stove Portable',     'Butane portable camping stove',          'B019', 44.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P139', 'SKU-0139', 'First Aid Kit',              'Comprehensive first aid kit 100-piece',  'B004', 24.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P140', 'SKU-0140', 'Dry Bag 20L',               'Waterproof roll-top dry bag',            'B004', 19.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P141', 'SKU-0141', 'Tennis Racket',              'Carbon fiber tennis racket',             'B010', 79.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P142', 'SKU-0142', 'Badminton Set',              'Outdoor badminton set 4-player',         'B010', 34.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P143', 'SKU-0143', 'Climbing Chalk Bag',         'Drawstring chalk bag for climbing',      'B019', 14.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P144', 'SKU-0144', 'Compression Socks',          'Graduated compression running socks',    'B010', 14.99,  0.060, true,  '2025-10-01 06:00:00+00'),
  ('P145', 'SKU-0145', 'Ab Roller Wheel',            'Core exercise ab wheel roller',          'B010', 19.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P146', 'SKU-0146', 'Swim Goggles',               'Anti-fog UV protection swim goggles',    'B010', 14.99,  0.050, true,  '2025-10-01 06:00:00+00'),
  ('P147', 'SKU-0147', 'Camping Chair Folding',      'Lightweight folding camp chair',         'B019', 39.99,  2.500, true,  '2025-10-01 06:00:00+00'),
  ('P148', 'SKU-0148', 'Kayak Paddle',               'Lightweight aluminum kayak paddle',      'B004', 49.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P149', 'SKU-0149', 'Bike Lock Cable',            'Heavy duty cable bike lock',             'B019', 24.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P150', 'SKU-0150', 'Skateboard Complete',        'Complete street skateboard',             'B009', 59.99,  2.500, true,  '2025-10-01 06:00:00+00'),

  -- Beauty & Health (P151-P170)
  ('P151', 'SKU-0151', 'Daily Moisturizer SPF30',    'Lightweight daily moisturizer with SPF', 'B012', 24.99,  0.120, true,  '2025-10-01 06:00:00+00'),
  ('P152', 'SKU-0152', 'Vitamin C Serum',            'Brightening vitamin C face serum',       'B012', 34.99,  0.060, true,  '2025-10-01 06:00:00+00'),
  ('P153', 'SKU-0153', 'Retinol Night Cream',        'Anti-aging retinol cream',               'B011', 44.99,  0.080, true,  '2025-10-01 06:00:00+00'),
  ('P154', 'SKU-0154', 'Gentle Cleanser',            'Fragrance-free face cleanser',           'B012', 14.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P155', 'SKU-0155', 'Lip Balm 3-Pack',            'Organic lip balm trio',                  'B004', 9.99,   0.030, true,  '2025-10-01 06:00:00+00'),
  ('P156', 'SKU-0156', 'Hair Oil Treatment',         'Argan oil hair treatment',               'B012', 19.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P157', 'SKU-0157', 'Multivitamin 90ct',          'Daily multivitamin capsules',            'B004', 19.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P158', 'SKU-0158', 'Omega-3 Fish Oil',           'High-potency omega-3 capsules',          'B004', 24.99,  0.250, true,  '2025-10-01 06:00:00+00'),
  ('P159', 'SKU-0159', 'Protein Powder Vanilla',     'Whey protein vanilla 2lb',               'B004', 34.99,  0.910, true,  '2025-10-01 06:00:00+00'),
  ('P160', 'SKU-0160', 'Collagen Peptides',          'Unflavored collagen powder',             'B004', 29.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P161', 'SKU-0161', 'Bamboo Toothbrush 4-Pack',   'Biodegradable bamboo toothbrushes',      'B004', 7.99,   0.060, true,  '2025-10-01 06:00:00+00'),
  ('P162', 'SKU-0162', 'Essential Oil Set',           'Aromatherapy oils 6-pack',               'B004', 24.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P163', 'SKU-0163', 'Hand Cream Lavender',        'Moisturizing lavender hand cream',       'B012', 12.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P164', 'SKU-0164', 'Nail Care Kit',              '7-piece manicure set in case',           'B014', 19.99,  0.150, true,  '2025-10-01 06:00:00+00'),
  ('P165', 'SKU-0165', 'Body Lotion Shea',           'Shea butter body lotion 16oz',           'B012', 14.99,  0.480, true,  '2025-10-01 06:00:00+00'),
  ('P166', 'SKU-0166', 'Facial Roller Jade',         'Natural jade face roller',               'B018', 19.99,  0.120, true,  '2025-10-01 06:00:00+00'),
  ('P167', 'SKU-0167', 'Dry Shampoo Spray',          'Volumizing dry shampoo',                 'B012', 11.99,  0.150, true,  '2025-10-01 06:00:00+00'),
  ('P168', 'SKU-0168', 'Sleep Mask Silk',             'Pure silk sleep eye mask',               'B018', 14.99,  0.030, true,  '2025-10-01 06:00:00+00'),
  ('P169', 'SKU-0169', 'Electric Toothbrush',        'Sonic electric toothbrush',              'B005', 49.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P170', 'SKU-0170', 'Shower Filter',              'Water softening shower filter',          'B004', 29.99,  0.400, true,  '2025-10-01 06:00:00+00'),

  -- Books & Media (P171-P180)
  ('P171', 'SKU-0171', 'Sci-Fi Novel Collection',    'Award-winning sci-fi boxed set',         'B016', 49.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P172', 'SKU-0172', 'Biography: Innovators',      'Profiles of tech innovators',            'B016', 24.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P173', 'SKU-0173', 'Fantasy Epic Trilogy',       'Dark fantasy trilogy set',               'B016', 39.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P174', 'SKU-0174', 'Cookbook Mediterranean',      'Mediterranean recipes cookbook',          'B014', 29.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P175', 'SKU-0175', 'Self-Help Mindfulness',      'Practical mindfulness guide',            'B016', 16.99,  0.400, true,  '2025-10-01 06:00:00+00'),

  -- Food & Beverage (P176-P185)
  ('P176', 'SKU-0176', 'Single Origin Coffee Beans', 'Ethiopian Yirgacheffe 12oz',             'B006', 18.99,  0.340, true,  '2025-10-01 06:00:00+00'),
  ('P177', 'SKU-0177', 'Matcha Green Tea Powder',    'Ceremonial grade matcha 100g',           'B008', 29.99,  0.100, true,  '2025-10-01 06:00:00+00'),
  ('P178', 'SKU-0178', 'Granola Bars Variety',       'Organic granola bars 12-pack',           'B004', 14.99,  0.480, true,  '2025-10-01 06:00:00+00'),
  ('P179', 'SKU-0179', 'Dark Chocolate 85%',         'Premium dark chocolate bar 100g',        'B014', 5.99,   0.100, true,  '2025-10-01 06:00:00+00'),
  ('P180', 'SKU-0180', 'Herbal Tea Sampler',         'Caffeine-free herbal teas 20ct',         'B008', 12.99,  0.150, true,  '2025-10-01 06:00:00+00'),
  ('P181', 'SKU-0181', 'Cold Brew Concentrate',      'Bottled cold brew 32oz',                 'B006', 14.99,  0.960, true,  '2025-10-01 06:00:00+00'),
  ('P182', 'SKU-0182', 'Trail Mix Bag',              'Nut and dried fruit trail mix 1lb',      'B004', 9.99,   0.454, true,  '2025-10-01 06:00:00+00'),
  ('P183', 'SKU-0183', 'Olive Oil Extra Virgin',     'Cold-pressed EVOO 500ml',                'B014', 16.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P184', 'SKU-0184', 'Honey Raw Organic',          'Raw organic wildflower honey 16oz',      'B004', 12.99,  0.454, true,  '2025-10-01 06:00:00+00'),
  ('P185', 'SKU-0185', 'Sparkling Water 12-Pack',    'Natural sparkling mineral water',        'B006', 11.99,  4.200, true,  '2025-10-01 06:00:00+00'),

  -- Toys & Games (P186-P200)
  ('P186', 'SKU-0186', 'Strategy Board Game',        'Euro-style strategy board game',         'B015', 44.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P187', 'SKU-0187', '1000-Piece Jigsaw Puzzle',   'Landscape 1000-piece puzzle',            'B015', 19.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P188', 'SKU-0188', 'Card Game Party',            'Party card game for 4-10 players',       'B015', 24.99,  0.400, true,  '2025-10-01 06:00:00+00'),
  ('P189', 'SKU-0189', 'Wooden Block Set',           'Classic wooden building blocks 50pc',    'B008', 29.99,  1.500, true,  '2025-10-01 06:00:00+00'),
  ('P190', 'SKU-0190', 'RC Car Off-Road',            'Remote control off-road car',            'B017', 39.99,  0.800, true,  '2025-10-01 06:00:00+00'),
  ('P191', 'SKU-0191', 'Art Supply Set',             'Drawing and painting kit 80-piece',      'B018', 34.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P192', 'SKU-0192', 'Science Kit Chemistry',      'Home chemistry experiment kit',          'B004', 29.99,  1.000, true,  '2025-10-01 06:00:00+00'),
  ('P193', 'SKU-0193', 'Trivia Game Deluxe',         '3000-question trivia game',              'B015', 29.99,  0.600, true,  '2025-10-01 06:00:00+00'),
  ('P194', 'SKU-0194', '3D Puzzle Monument',         '3D monument building puzzle',            'B018', 24.99,  0.500, true,  '2025-10-01 06:00:00+00'),
  ('P195', 'SKU-0195', 'Drone Mini',                 'Pocket-size camera drone',               'B017', 79.99,  0.200, true,  '2025-10-01 06:00:00+00'),
  ('P196', 'SKU-0196', 'Magnetic Tiles Set',         'Magnetic building tiles 60-piece',       'B018', 39.99,  1.200, true,  '2025-10-01 06:00:00+00'),
  ('P197', 'SKU-0197', 'Cooperative Board Game',     'Cooperative strategy board game',        'B015', 34.99,  1.000, true,  '2025-10-01 06:00:00+00'),
  ('P198', 'SKU-0198', 'Model Airplane Kit',         'Balsa wood model airplane kit',          'B008', 19.99,  0.300, true,  '2025-10-01 06:00:00+00'),
  ('P199', 'SKU-0199', 'Rubik Cube Speed',           'Competition speed cube 3x3',             'B018', 12.99,  0.080, true,  '2025-10-01 06:00:00+00'),
  ('P200', 'SKU-0200', 'Plush Teddy Bear',           'Soft plush teddy bear 18 inch',          'B003', 24.99,  0.500, true,  '2025-10-01 06:00:00+00')
ON CONFLICT DO NOTHING;

-- === Product Categories (400 mappings) ===
-- Each product gets 2 category mappings (leaf + parent)

INSERT INTO product_categories (product_id, category_id) VALUES
  -- Audio products -> Electronics, Audio, Headphones
  ('P001','CAT001'), ('P001','CAT009'), ('P002','CAT001'), ('P002','CAT025'),
  ('P003','CAT001'), ('P003','CAT009'), ('P004','CAT001'), ('P004','CAT025'),
  ('P005','CAT001'), ('P005','CAT025'), ('P006','CAT001'), ('P006','CAT009'),
  ('P007','CAT001'), ('P007','CAT009'), ('P008','CAT001'), ('P008','CAT009'),
  ('P009','CAT001'), ('P009','CAT009'), ('P010','CAT001'), ('P010','CAT025'),
  ('P011','CAT001'), ('P011','CAT025'), ('P012','CAT001'), ('P012','CAT009'),
  ('P013','CAT001'), ('P013','CAT025'), ('P014','CAT001'), ('P014','CAT009'),
  ('P015','CAT001'), ('P015','CAT009'), ('P016','CAT001'), ('P016','CAT009'),
  ('P017','CAT001'), ('P017','CAT025'), ('P018','CAT001'), ('P018','CAT009'),
  ('P019','CAT001'), ('P019','CAT009'), ('P020','CAT001'), ('P020','CAT009'),
  -- Computers -> Electronics, Computers, Laptops
  ('P021','CAT001'), ('P021','CAT026'), ('P022','CAT001'), ('P022','CAT010'),
  ('P023','CAT001'), ('P023','CAT010'), ('P024','CAT001'), ('P024','CAT010'),
  ('P025','CAT001'), ('P025','CAT010'), ('P026','CAT001'), ('P026','CAT010'),
  ('P027','CAT001'), ('P027','CAT010'), ('P028','CAT001'), ('P028','CAT010'),
  ('P029','CAT001'), ('P029','CAT010'), ('P030','CAT001'), ('P030','CAT010'),
  ('P031','CAT001'), ('P031','CAT010'), ('P032','CAT001'), ('P032','CAT010'),
  ('P033','CAT001'), ('P033','CAT010'), ('P034','CAT001'), ('P034','CAT010'),
  ('P035','CAT001'), ('P035','CAT010'), ('P036','CAT001'), ('P036','CAT010'),
  ('P037','CAT001'), ('P037','CAT010'), ('P038','CAT001'), ('P038','CAT010'),
  ('P039','CAT001'), ('P039','CAT010'), ('P040','CAT002'), ('P040','CAT010'),
  -- Men clothing -> Clothing, Men, T-Shirts
  ('P041','CAT002'), ('P041','CAT027'), ('P042','CAT002'), ('P042','CAT011'),
  ('P043','CAT002'), ('P043','CAT011'), ('P044','CAT002'), ('P044','CAT011'),
  ('P045','CAT002'), ('P045','CAT011'), ('P046','CAT002'), ('P046','CAT011'),
  ('P047','CAT002'), ('P047','CAT011'), ('P048','CAT002'), ('P048','CAT011'),
  ('P049','CAT002'), ('P049','CAT011'), ('P050','CAT002'), ('P050','CAT011'),
  ('P051','CAT002'), ('P051','CAT011'), ('P052','CAT002'), ('P052','CAT011'),
  ('P053','CAT002'), ('P053','CAT011'), ('P054','CAT002'), ('P054','CAT011'),
  ('P055','CAT002'), ('P055','CAT011'), ('P056','CAT002'), ('P056','CAT011'),
  ('P057','CAT002'), ('P057','CAT011'), ('P058','CAT002'), ('P058','CAT011'),
  ('P059','CAT002'), ('P059','CAT027'), ('P060','CAT002'), ('P060','CAT011'),
  -- Women clothing -> Clothing, Women, Dresses
  ('P061','CAT002'), ('P061','CAT028'), ('P062','CAT002'), ('P062','CAT012'),
  ('P063','CAT002'), ('P063','CAT012'), ('P064','CAT002'), ('P064','CAT012'),
  ('P065','CAT002'), ('P065','CAT012'), ('P066','CAT002'), ('P066','CAT012'),
  ('P067','CAT002'), ('P067','CAT012'), ('P068','CAT002'), ('P068','CAT012'),
  ('P069','CAT002'), ('P069','CAT012'), ('P070','CAT002'), ('P070','CAT012'),
  ('P071','CAT002'), ('P071','CAT028'), ('P072','CAT002'), ('P072','CAT012'),
  ('P073','CAT002'), ('P073','CAT012'), ('P074','CAT002'), ('P074','CAT012'),
  ('P075','CAT002'), ('P075','CAT012'), ('P076','CAT002'), ('P076','CAT012'),
  ('P077','CAT002'), ('P077','CAT012'), ('P078','CAT002'), ('P078','CAT012'),
  ('P079','CAT002'), ('P079','CAT012'), ('P080','CAT002'), ('P080','CAT012'),
  -- Home & Kitchen -> Home & Kitchen, Kitchen, Cookware / Living Room
  ('P081','CAT003'), ('P081','CAT029'), ('P082','CAT003'), ('P082','CAT029'),
  ('P083','CAT003'), ('P083','CAT029'), ('P084','CAT003'), ('P084','CAT013'),
  ('P085','CAT003'), ('P085','CAT013'), ('P086','CAT003'), ('P086','CAT013'),
  ('P087','CAT003'), ('P087','CAT013'), ('P088','CAT003'), ('P088','CAT029'),
  ('P089','CAT003'), ('P089','CAT029'), ('P090','CAT003'), ('P090','CAT013'),
  ('P091','CAT003'), ('P091','CAT014'), ('P092','CAT003'), ('P092','CAT014'),
  ('P093','CAT003'), ('P093','CAT014'), ('P094','CAT003'), ('P094','CAT014'),
  ('P095','CAT003'), ('P095','CAT014'), ('P096','CAT003'), ('P096','CAT014'),
  ('P097','CAT003'), ('P097','CAT014'), ('P098','CAT003'), ('P098','CAT014'),
  ('P099','CAT003'), ('P099','CAT014'), ('P100','CAT003'), ('P100','CAT014'),
  ('P101','CAT003'), ('P101','CAT029'), ('P102','CAT003'), ('P102','CAT013'),
  ('P103','CAT003'), ('P103','CAT013'), ('P104','CAT003'), ('P104','CAT013'),
  ('P105','CAT003'), ('P105','CAT013'), ('P106','CAT003'), ('P106','CAT013'),
  ('P107','CAT003'), ('P107','CAT013'), ('P108','CAT003'), ('P108','CAT014'),
  ('P109','CAT003'), ('P109','CAT014'), ('P110','CAT003'), ('P110','CAT013'),
  ('P111','CAT003'), ('P111','CAT029'), ('P112','CAT003'), ('P112','CAT013'),
  ('P113','CAT003'), ('P113','CAT013'), ('P114','CAT003'), ('P114','CAT013'),
  ('P115','CAT003'), ('P115','CAT013'), ('P116','CAT003'), ('P116','CAT013'),
  ('P117','CAT003'), ('P117','CAT013'), ('P118','CAT003'), ('P118','CAT013'),
  ('P119','CAT003'), ('P119','CAT029'), ('P120','CAT003'), ('P120','CAT013'),
  -- Sports -> Sports & Outdoors, Fitness/Camping, Yoga/Tents
  ('P121','CAT004'), ('P121','CAT031'), ('P122','CAT004'), ('P122','CAT015'),
  ('P123','CAT004'), ('P123','CAT015'), ('P124','CAT004'), ('P124','CAT015'),
  ('P125','CAT004'), ('P125','CAT031'), ('P126','CAT004'), ('P126','CAT016'),
  ('P127','CAT004'), ('P127','CAT032'), ('P128','CAT004'), ('P128','CAT016'),
  ('P129','CAT004'), ('P129','CAT016'), ('P130','CAT004'), ('P130','CAT015'),
  ('P131','CAT004'), ('P131','CAT016'), ('P132','CAT004'), ('P132','CAT016'),
  ('P133','CAT004'), ('P133','CAT015'), ('P134','CAT004'), ('P134','CAT031'),
  ('P135','CAT004'), ('P135','CAT015'), ('P136','CAT004'), ('P136','CAT015'),
  ('P137','CAT004'), ('P137','CAT015'), ('P138','CAT004'), ('P138','CAT016'),
  ('P139','CAT004'), ('P139','CAT016'), ('P140','CAT004'), ('P140','CAT016'),
  ('P141','CAT004'), ('P141','CAT015'), ('P142','CAT004'), ('P142','CAT015'),
  ('P143','CAT004'), ('P143','CAT016'), ('P144','CAT004'), ('P144','CAT015'),
  ('P145','CAT004'), ('P145','CAT015'), ('P146','CAT004'), ('P146','CAT015'),
  ('P147','CAT004'), ('P147','CAT016'), ('P148','CAT004'), ('P148','CAT016'),
  ('P149','CAT004'), ('P149','CAT015'), ('P150','CAT004'), ('P150','CAT015'),
  -- Beauty & Health
  ('P151','CAT005'), ('P151','CAT033'), ('P152','CAT005'), ('P152','CAT017'),
  ('P153','CAT005'), ('P153','CAT033'), ('P154','CAT005'), ('P154','CAT017'),
  ('P155','CAT005'), ('P155','CAT017'), ('P156','CAT005'), ('P156','CAT017'),
  ('P157','CAT005'), ('P157','CAT034'), ('P158','CAT005'), ('P158','CAT034'),
  ('P159','CAT005'), ('P159','CAT018'), ('P160','CAT005'), ('P160','CAT018'),
  ('P161','CAT005'), ('P161','CAT017'), ('P162','CAT005'), ('P162','CAT017'),
  ('P163','CAT005'), ('P163','CAT017'), ('P164','CAT005'), ('P164','CAT017'),
  ('P165','CAT005'), ('P165','CAT017'), ('P166','CAT005'), ('P166','CAT017'),
  ('P167','CAT005'), ('P167','CAT017'), ('P168','CAT005'), ('P168','CAT017'),
  ('P169','CAT005'), ('P169','CAT017'), ('P170','CAT005'), ('P170','CAT017'),
  -- Books & Media
  ('P171','CAT006'), ('P171','CAT035'), ('P172','CAT006'), ('P172','CAT036'),
  ('P173','CAT006'), ('P173','CAT019'), ('P174','CAT006'), ('P174','CAT020'),
  ('P175','CAT006'), ('P175','CAT020'),
  -- Food & Beverage
  ('P176','CAT007'), ('P176','CAT037'), ('P177','CAT007'), ('P177','CAT021'),
  ('P178','CAT007'), ('P178','CAT038'), ('P179','CAT007'), ('P179','CAT022'),
  ('P180','CAT007'), ('P180','CAT021'), ('P181','CAT007'), ('P181','CAT037'),
  ('P182','CAT007'), ('P182','CAT022'), ('P183','CAT007'), ('P183','CAT022'),
  ('P184','CAT007'), ('P184','CAT022'), ('P185','CAT007'), ('P185','CAT021'),
  -- Toys & Games
  ('P186','CAT008'), ('P186','CAT039'), ('P187','CAT008'), ('P187','CAT040'),
  ('P188','CAT008'), ('P188','CAT023'), ('P189','CAT008'), ('P189','CAT023'),
  ('P190','CAT008'), ('P190','CAT023'), ('P191','CAT008'), ('P191','CAT023'),
  ('P192','CAT008'), ('P192','CAT023'), ('P193','CAT008'), ('P193','CAT039'),
  ('P194','CAT008'), ('P194','CAT040'), ('P195','CAT008'), ('P195','CAT023'),
  ('P196','CAT008'), ('P196','CAT023'), ('P197','CAT008'), ('P197','CAT039'),
  ('P198','CAT008'), ('P198','CAT023'), ('P199','CAT008'), ('P199','CAT024'),
  ('P200','CAT008'), ('P200','CAT023')
ON CONFLICT DO NOTHING;

-- === Product Suppliers (60 mappings with is_primary flags) ===

INSERT INTO product_suppliers (product_id, supplier_id, cost, is_primary) VALUES
  ('P001','SUP001', 60.00, true),  ('P001','SUP008', 52.00, false),
  ('P002','SUP001', 100.00, true), ('P002','SUP005', 95.00, false),
  ('P003','SUP001', 24.00, true),  ('P004','SUP005', 50.00, true),
  ('P005','SUP005', 80.00, true),  ('P006','SUP001', 130.00, true),
  ('P007','SUP001', 35.00, true),  ('P008','SUP005', 70.00, true),
  ('P009','SUP012', 110.00, true), ('P010','SUP001', 80.00, true),
  ('P011','SUP008', 15.00, true),  ('P012','SUP001', 40.00, true),
  ('P013','SUP008', 30.00, true),  ('P021','SUP001', 200.00, true),
  ('P022','SUP005', 50.00, true),  ('P023','SUP003', 20.00, true),
  ('P025','SUP008', 140.00, true), ('P028','SUP008', 35.00, true),
  ('P034','SUP005', 80.00, true),  ('P037','SUP001', 110.00, true),
  ('P041','SUP010', 12.00, true),  ('P041','SUP011', 13.00, false),
  ('P042','SUP010', 20.00, true),  ('P043','SUP012', 35.00, true),
  ('P044','SUP012', 24.00, true),  ('P045','SUP010', 32.00, true),
  ('P061','SUP011', 28.00, true),  ('P062','SUP007', 24.00, true),
  ('P063','SUP011', 36.00, true),  ('P064','SUP010', 32.00, true),
  ('P065','SUP007', 22.00, true),  ('P066','SUP012', 60.00, true),
  ('P081','SUP006', 5.00, true),   ('P082','SUP003', 16.00, true),
  ('P083','SUP003', 32.00, true),  ('P085','SUP003', 14.00, true),
  ('P087','SUP003', 24.00, true),  ('P101','SUP011', 36.00, true),
  ('P119','SUP011', 100.00, true), ('P120','SUP003', 28.00, true),
  ('P121','SUP007', 16.00, true),  ('P122','SUP007', 8.00, true),
  ('P123','SUP003', 60.00, true),  ('P126','SUP003', 36.00, true),
  ('P127','SUP003', 72.00, true),  ('P128','SUP003', 32.00, true),
  ('P151','SUP010', 10.00, true),  ('P152','SUP010', 14.00, true),
  ('P157','SUP002', 8.00, true),   ('P158','SUP002', 10.00, true),
  ('P171','SUP012', 20.00, true),  ('P176','SUP014', 7.00, true),
  ('P177','SUP006', 12.00, true),  ('P186','SUP012', 18.00, true),
  ('P187','SUP012', 8.00, true),   ('P190','SUP008', 16.00, true),
  ('P195','SUP008', 32.00, true),  ('P200','SUP009', 10.00, true)
ON CONFLICT DO NOTHING;

-- === Orders (ORD001–ORD500) ===
-- Spread across 2025-10-01 to 2026-03-25, mixed statuses

DO $$
DECLARE
  i INT;
  cid TEXT;
  st TEXT;
  amt NUMERIC;
  odate TIMESTAMPTZ;
  sdate TIMESTAMPTZ;
  ddate TIMESTAMPTZ;
  statuses TEXT[] := ARRAY['pending','confirmed','shipped','delivered','cancelled','returned'];
  base_date TIMESTAMPTZ := '2025-10-01 00:00:00+00';
BEGIN
  FOR i IN 1..500 LOOP
    -- Deterministic customer: cycle C001-C100
    cid := 'C' || LPAD(((i - 1) % 100 + 1)::TEXT, 3, '0');
    -- Deterministic status: delivered 50%, shipped 20%, confirmed 10%, pending 10%, cancelled 7%, returned 3%
    st := CASE
      WHEN i % 100 <= 49 THEN 'delivered'
      WHEN i % 100 <= 69 THEN 'shipped'
      WHEN i % 100 <= 79 THEN 'confirmed'
      WHEN i % 100 <= 89 THEN 'pending'
      WHEN i % 100 <= 96 THEN 'cancelled'
      ELSE 'returned'
    END;
    -- Deterministic amount: base on i
    amt := 20.00 + (i * 7 % 480)::NUMERIC + ((i * 13 % 100)::NUMERIC / 100);
    -- Deterministic date: spread over ~177 days
    odate := base_date + ((i - 1) * 8 || ' hours')::INTERVAL;
    -- shipped_at and delivered_at for relevant statuses
    sdate := CASE WHEN st IN ('shipped','delivered','returned') THEN odate + '2 days'::INTERVAL ELSE NULL END;
    ddate := CASE WHEN st IN ('delivered','returned') THEN odate + '5 days'::INTERVAL ELSE NULL END;

    INSERT INTO orders (id, customer_id, status, total_amount, currency, ordered_at, shipped_at, delivered_at)
    VALUES (
      'ORD' || LPAD(i::TEXT, 3, '0'),
      cid, st, amt, 'USD', odate, sdate, ddate
    ) ON CONFLICT DO NOTHING;
  END LOOP;
END $$;

-- === Order Items (OI0001–OI1200) ===
-- ~2-3 items per order, deterministic product + quantity + price

DO $$
DECLARE
  oi_idx INT := 1;
  ord_idx INT;
  item_count INT;
  j INT;
  pid TEXT;
  qty INT;
  uprice NUMERIC;
  disc NUMERIC;
BEGIN
  FOR ord_idx IN 1..500 LOOP
    -- 2 or 3 items per order (pattern: every 3rd order gets 3 items, rest get 2)
    item_count := CASE WHEN ord_idx % 3 = 0 THEN 3 ELSE 2 END;
    FOR j IN 1..item_count LOOP
      -- Deterministic product: cycle across P001-P200
      pid := 'P' || LPAD((((ord_idx - 1) * 3 + j - 1) % 200 + 1)::TEXT, 3, '0');
      qty := (ord_idx + j) % 4 + 1;  -- 1 to 4
      uprice := 10.00 + ((ord_idx * 7 + j * 13) % 490)::NUMERIC + 0.99;
      disc := CASE WHEN (ord_idx + j) % 7 = 0 THEN 10.00 WHEN (ord_idx + j) % 11 = 0 THEN 15.00 ELSE 0.00 END;

      INSERT INTO order_items (id, order_id, product_id, quantity, unit_price, discount_pct)
      VALUES (
        'OI' || LPAD(oi_idx::TEXT, 4, '0'),
        'ORD' || LPAD(ord_idx::TEXT, 3, '0'),
        pid, qty, uprice, disc
      ) ON CONFLICT DO NOTHING;

      oi_idx := oi_idx + 1;
      EXIT WHEN oi_idx > 1200;
    END LOOP;
    EXIT WHEN oi_idx > 1200;
  END LOOP;
END $$;

-- === Reviews (REV001–REV300) ===
-- Ratings skewed toward 4-5

DO $$
DECLARE
  i INT;
  cid TEXT;
  pid TEXT;
  rat INT;
  titles TEXT[] := ARRAY[
    'Great product!', 'Excellent quality', 'Good value', 'Decent purchase',
    'Love it!', 'Highly recommend', 'Solid build', 'Worth every penny',
    'Better than expected', 'Not bad', 'Could be better', 'Disappointing',
    'Amazing!', 'Perfect gift', 'Very satisfied', 'Okay for the price',
    'Fantastic quality', 'Exceeded expectations', 'Will buy again', 'Pretty good'
  ];
  bodies TEXT[] := ARRAY[
    'Works exactly as described. Very happy with this purchase.',
    'The quality is outstanding for this price point.',
    'Arrived quickly and well-packaged. No complaints.',
    'Decent product but nothing special. Does the job.',
    'Absolutely love this! Would definitely recommend to friends.',
    'Good quality materials and construction throughout.',
    'A bit pricey but the quality justifies the cost.',
    'Exactly what I was looking for. Perfect fit.',
    'Surprised by how well-made this is. Great find.',
    'Met my expectations. Solid everyday item.'
  ];
BEGIN
  FOR i IN 1..300 LOOP
    cid := 'C' || LPAD(((i - 1) % 100 + 1)::TEXT, 3, '0');
    pid := 'P' || LPAD(((i - 1) % 200 + 1)::TEXT, 3, '0');
    -- Skewed ratings: ~10% get 1-2, ~20% get 3, ~35% get 4, ~35% get 5
    rat := CASE
      WHEN i % 20 = 0 THEN 1
      WHEN i % 20 IN (1, 10) THEN 2
      WHEN i % 20 IN (2, 3, 11, 12) THEN 3
      WHEN i % 20 IN (4, 5, 6, 13, 14, 15, 16) THEN 4
      ELSE 5
    END;

    INSERT INTO reviews (id, customer_id, product_id, rating, title, body, helpful_count, created_at)
    VALUES (
      'REV' || LPAD(i::TEXT, 3, '0'),
      cid, pid, rat,
      titles[(i - 1) % 20 + 1],
      bodies[(i - 1) % 10 + 1],
      (i * 3) % 25,
      '2025-10-05 00:00:00+00'::TIMESTAMPTZ + ((i - 1) * 14 || ' hours')::INTERVAL
    ) ON CONFLICT DO NOTHING;
  END LOOP;
END $$;

-- === Inventory (400 records across warehouses) ===

DO $$
DECLARE
  i INT;
  wh TEXT;
  pid TEXT;
  qty INT;
  wh_ids TEXT[] := ARRAY['WH001','WH002','WH003','WH004','WH005','WH006','WH007','WH008'];
  restock TIMESTAMPTZ;
BEGIN
  FOR i IN 1..400 LOOP
    wh := wh_ids[(i - 1) % 8 + 1];
    pid := 'P' || LPAD(((i - 1) % 200 + 1)::TEXT, 3, '0');
    qty := 10 + (i * 7) % 490;
    restock := '2025-10-01 00:00:00+00'::TIMESTAMPTZ + ((i - 1) * 5 || ' hours')::INTERVAL;

    INSERT INTO inventory (warehouse_id, product_id, quantity, last_restocked_at)
    VALUES (wh, pid, qty, restock)
    ON CONFLICT DO NOTHING;
  END LOOP;
END $$;

-- === Campaigns (CAMP01–CAMP06) ===

INSERT INTO campaigns (id, name, type, budget, start_date, end_date) VALUES
  ('CAMP01', 'Holiday Season Sale',       'seasonal',   50000.00, '2025-11-25', '2025-12-31'),
  ('CAMP02', 'New Year Flash Sale',       'flash_sale', 20000.00, '2026-01-01', '2026-01-07'),
  ('CAMP03', 'Spring Bundle Deals',       'bundle',     30000.00, '2026-03-01', '2026-03-31'),
  ('CAMP04', 'Loyalty Rewards Q1',        'loyalty',    15000.00, '2026-01-01', '2026-03-31'),
  ('CAMP05', 'Electronics Discount Week', 'discount',   25000.00, '2025-11-15', '2025-11-22'),
  ('CAMP06', 'Valentine Special',         'seasonal',   10000.00, '2026-02-07', '2026-02-14')
ON CONFLICT DO NOTHING;

-- === Campaign Products (30 mappings) ===

INSERT INTO campaign_products (campaign_id, product_id, discount_pct) VALUES
  -- Holiday Season (8 products)
  ('CAMP01', 'P001', 20.00), ('CAMP01', 'P021', 15.00), ('CAMP01', 'P041', 25.00),
  ('CAMP01', 'P081', 30.00), ('CAMP01', 'P121', 20.00), ('CAMP01', 'P151', 15.00),
  ('CAMP01', 'P171', 10.00), ('CAMP01', 'P186', 20.00),
  -- Flash Sale (5 products)
  ('CAMP02', 'P002', 35.00), ('CAMP02', 'P025', 30.00), ('CAMP02', 'P119', 25.00),
  ('CAMP02', 'P066', 20.00), ('CAMP02', 'P127', 30.00),
  -- Spring Bundle (5 products)
  ('CAMP03', 'P085', 15.00), ('CAMP03', 'P086', 15.00), ('CAMP03', 'P176', 10.00),
  ('CAMP03', 'P177', 10.00), ('CAMP03', 'P120', 15.00),
  -- Loyalty Rewards (4 products)
  ('CAMP04', 'P043', 10.00), ('CAMP04', 'P083', 10.00), ('CAMP04', 'P101', 12.00),
  ('CAMP04', 'P069', 10.00),
  -- Electronics Discount (5 products)
  ('CAMP05', 'P006', 20.00), ('CAMP05', 'P012', 25.00), ('CAMP05', 'P022', 15.00),
  ('CAMP05', 'P028', 20.00), ('CAMP05', 'P037', 15.00),
  -- Valentine Special (3 products)
  ('CAMP06', 'P063', 15.00), ('CAMP06', 'P080', 20.00), ('CAMP06', 'P092', 25.00)
ON CONFLICT DO NOTHING;

-- === Customer Segment Members (80 mappings) ===

INSERT INTO customer_segment_members (customer_id, segment_id, assigned_at) VALUES
  -- Bronze (20 members)
  ('C004','SEG001','2025-10-05 10:00:00+00'), ('C006','SEG001','2025-10-05 10:00:00+00'),
  ('C008','SEG001','2025-10-05 10:00:00+00'), ('C011','SEG001','2025-10-05 10:00:00+00'),
  ('C014','SEG001','2025-10-05 10:00:00+00'), ('C017','SEG001','2025-10-06 10:00:00+00'),
  ('C019','SEG001','2025-10-06 10:00:00+00'), ('C021','SEG001','2025-10-06 10:00:00+00'),
  ('C023','SEG001','2025-10-06 10:00:00+00'), ('C025','SEG001','2025-10-06 10:00:00+00'),
  ('C033','SEG001','2025-10-07 10:00:00+00'), ('C035','SEG001','2025-10-07 10:00:00+00'),
  ('C039','SEG001','2025-10-07 10:00:00+00'), ('C043','SEG001','2025-10-07 10:00:00+00'),
  ('C051','SEG001','2025-10-08 10:00:00+00'), ('C057','SEG001','2025-10-08 10:00:00+00'),
  ('C059','SEG001','2025-10-08 10:00:00+00'), ('C063','SEG001','2025-10-08 10:00:00+00'),
  ('C065','SEG001','2025-10-09 10:00:00+00'), ('C069','SEG001','2025-10-09 10:00:00+00'),
  -- Silver (20 members)
  ('C003','SEG002','2025-10-05 11:00:00+00'), ('C007','SEG002','2025-10-05 11:00:00+00'),
  ('C010','SEG002','2025-10-05 11:00:00+00'), ('C012','SEG002','2025-10-05 11:00:00+00'),
  ('C015','SEG002','2025-10-06 11:00:00+00'), ('C020','SEG002','2025-10-06 11:00:00+00'),
  ('C024','SEG002','2025-10-06 11:00:00+00'), ('C028','SEG002','2025-10-06 11:00:00+00'),
  ('C032','SEG002','2025-10-07 11:00:00+00'), ('C036','SEG002','2025-10-07 11:00:00+00'),
  ('C042','SEG002','2025-10-07 11:00:00+00'), ('C046','SEG002','2025-10-07 11:00:00+00'),
  ('C050','SEG002','2025-10-08 11:00:00+00'), ('C052','SEG002','2025-10-08 11:00:00+00'),
  ('C056','SEG002','2025-10-08 11:00:00+00'), ('C060','SEG002','2025-10-08 11:00:00+00'),
  ('C064','SEG002','2025-10-09 11:00:00+00'), ('C068','SEG002','2025-10-09 11:00:00+00'),
  ('C072','SEG002','2025-10-09 11:00:00+00'), ('C086','SEG002','2025-10-09 11:00:00+00'),
  -- Gold (20 members)
  ('C002','SEG003','2025-10-05 12:00:00+00'), ('C005','SEG003','2025-10-05 12:00:00+00'),
  ('C009','SEG003','2025-10-05 12:00:00+00'), ('C013','SEG003','2025-10-05 12:00:00+00'),
  ('C018','SEG003','2025-10-06 12:00:00+00'), ('C022','SEG003','2025-10-06 12:00:00+00'),
  ('C026','SEG003','2025-10-06 12:00:00+00'), ('C030','SEG003','2025-10-06 12:00:00+00'),
  ('C034','SEG003','2025-10-07 12:00:00+00'), ('C038','SEG003','2025-10-07 12:00:00+00'),
  ('C044','SEG003','2025-10-07 12:00:00+00'), ('C048','SEG003','2025-10-07 12:00:00+00'),
  ('C054','SEG003','2025-10-08 12:00:00+00'), ('C058','SEG003','2025-10-08 12:00:00+00'),
  ('C062','SEG003','2025-10-08 12:00:00+00'), ('C066','SEG003','2025-10-08 12:00:00+00'),
  ('C070','SEG003','2025-10-09 12:00:00+00'), ('C074','SEG003','2025-10-09 12:00:00+00'),
  ('C088','SEG003','2025-10-09 12:00:00+00'), ('C092','SEG003','2025-10-09 12:00:00+00'),
  -- Platinum (12 members)
  ('C001','SEG004','2025-10-05 13:00:00+00'), ('C016','SEG004','2025-10-06 13:00:00+00'),
  ('C040','SEG004','2025-10-07 13:00:00+00'), ('C076','SEG004','2025-10-08 13:00:00+00'),
  ('C100','SEG004','2025-10-09 13:00:00+00'), ('C084','SEG004','2025-10-09 13:00:00+00'),
  ('C096','SEG004','2025-10-10 13:00:00+00'), ('C080','SEG004','2025-10-10 13:00:00+00'),
  ('C090','SEG004','2025-10-10 13:00:00+00'), ('C094','SEG004','2025-10-10 13:00:00+00'),
  ('C098','SEG004','2025-10-11 13:00:00+00'), ('C078','SEG004','2025-10-11 13:00:00+00'),
  -- VIP (8 members)
  ('C001','SEG005','2025-10-05 14:00:00+00'), ('C016','SEG005','2025-10-06 14:00:00+00'),
  ('C040','SEG005','2025-10-07 14:00:00+00'), ('C076','SEG005','2025-10-08 14:00:00+00'),
  ('C100','SEG005','2025-10-09 14:00:00+00'), ('C084','SEG005','2025-10-09 14:00:00+00'),
  ('C096','SEG005','2025-10-10 14:00:00+00'), ('C088','SEG005','2025-10-10 14:00:00+00')
ON CONFLICT DO NOTHING;

-- === Shipping Events (SE0001–SE0600) ===
-- Temporal sequences: for delivered orders, create label_created -> picked_up -> in_transit -> out_for_delivery -> delivered

DO $$
DECLARE
  se_idx INT := 1;
  ord_idx INT;
  odate TIMESTAMPTZ;
  base_date TIMESTAMPTZ := '2025-10-01 00:00:00+00';
  st TEXT;
  events TEXT[];
  ev_count INT;
  k INT;
  loc TEXT;
  locations TEXT[] := ARRAY[
    'Distribution Center A', 'Regional Hub East', 'Regional Hub West',
    'Sorting Facility', 'Local Post Office', 'Final Mile Hub',
    'Airport Cargo', 'Customs Clearance', 'City Depot', 'Customer Address'
  ];
BEGIN
  FOR ord_idx IN 1..500 LOOP
    -- Determine status (same logic as order generation)
    st := CASE
      WHEN ord_idx % 100 <= 49 THEN 'delivered'
      WHEN ord_idx % 100 <= 69 THEN 'shipped'
      WHEN ord_idx % 100 <= 79 THEN 'confirmed'
      WHEN ord_idx % 100 <= 89 THEN 'pending'
      WHEN ord_idx % 100 <= 96 THEN 'cancelled'
      ELSE 'returned'
    END;

    odate := base_date + ((ord_idx - 1) * 8 || ' hours')::INTERVAL;

    -- Only create shipping events for shipped/delivered/returned orders
    IF st IN ('delivered', 'returned') THEN
      events := ARRAY['label_created', 'picked_up', 'in_transit', 'out_for_delivery', 'delivered'];
    ELSIF st = 'shipped' THEN
      events := ARRAY['label_created', 'picked_up', 'in_transit'];
    ELSE
      CONTINUE;
    END IF;

    FOR k IN 1..array_length(events, 1) LOOP
      EXIT WHEN se_idx > 600;

      loc := locations[(ord_idx + k - 1) % 10 + 1];

      INSERT INTO shipping_events (id, order_id, event_type, location, occurred_at)
      VALUES (
        'SE' || LPAD(se_idx::TEXT, 4, '0'),
        'ORD' || LPAD(ord_idx::TEXT, 3, '0'),
        events[k],
        loc,
        odate + ((k * 12) || ' hours')::INTERVAL
      ) ON CONFLICT DO NOTHING;

      se_idx := se_idx + 1;
    END LOOP;

    EXIT WHEN se_idx > 600;
  END LOOP;
END $$;

-- === Product Recommendations (100 records) ===

DO $$
DECLARE
  i INT;
  src TEXT;
  tgt TEXT;
  sc NUMERIC;
BEGIN
  FOR i IN 1..100 LOOP
    src := 'P' || LPAD(((i - 1) % 200 + 1)::TEXT, 3, '0');
    -- Target is offset by a varying amount to avoid src = tgt
    tgt := 'P' || LPAD((((i - 1) % 200 + 1 + (i * 7 % 199) + 1 - 1) % 200 + 1)::TEXT, 3, '0');
    -- Ensure src != tgt
    IF src = tgt THEN
      tgt := 'P' || LPAD((((i - 1) % 200 + 3) % 200 + 1)::TEXT, 3, '0');
    END IF;
    sc := (0.500 + (i * 3 % 500)::NUMERIC / 1000.0);
    IF sc > 1.000 THEN sc := 0.999; END IF;

    INSERT INTO product_recommendations (source_product_id, target_product_id, score, algorithm)
    VALUES (src, tgt, sc, 'collaborative_filter')
    ON CONFLICT DO NOTHING;
  END LOOP;
END $$;

-- ============================================================
-- Seed complete.
-- Expected counts:
--   brands: 20, categories: 40, suppliers: 15, warehouses: 8
--   customer_segments: 5, customers: 100, products: 200
--   product_categories: ~400, product_suppliers: 60
--   orders: 500, order_items: 1200, reviews: 300
--   inventory: 400, campaigns: 6, campaign_products: 30
--   customer_segment_members: 80, shipping_events: 600
--   product_recommendations: 100
-- ============================================================
