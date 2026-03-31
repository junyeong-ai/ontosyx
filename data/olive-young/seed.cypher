// ============================================================
// Olive Young Knowledge Graph — Medium Scale Seed Data
// 100 products, 20 stores, 50 customers, ~200 transactions
// ============================================================

// --- Constraints & Indexes ---
CREATE CONSTRAINT IF NOT EXISTS FOR (p:Product) REQUIRE p.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (b:Brand) REQUIRE b.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (c:Category) REQUIRE c.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (s:Store) REQUIRE s.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (r:Region) REQUIRE r.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (cu:Customer) REQUIRE cu.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (t:Transaction) REQUIRE t.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (rv:Review) REQUIRE rv.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (pm:Promotion) REQUIRE pm.id IS UNIQUE;

// --- Regions ---
CREATE (:Region {id: 'reg-seoul', name: '서울', type: '특별시'});
CREATE (:Region {id: 'reg-gyeonggi', name: '경기', type: '도'});
CREATE (:Region {id: 'reg-busan', name: '부산', type: '광역시'});
CREATE (:Region {id: 'reg-daegu', name: '대구', type: '광역시'});
CREATE (:Region {id: 'reg-incheon', name: '인천', type: '광역시'});
CREATE (:Region {id: 'reg-daejeon', name: '대전', type: '광역시'});
CREATE (:Region {id: 'reg-gwangju', name: '광주', type: '광역시'});

// --- Categories ---
CREATE (:Category {id: 'cat-skincare', name: '스킨케어', parent: ''});
CREATE (:Category {id: 'cat-toner', name: '토너/스킨', parent: 'cat-skincare'});
CREATE (:Category {id: 'cat-serum', name: '세럼/에센스', parent: 'cat-skincare'});
CREATE (:Category {id: 'cat-cream', name: '크림/로션', parent: 'cat-skincare'});
CREATE (:Category {id: 'cat-suncare', name: '선케어', parent: 'cat-skincare'});
CREATE (:Category {id: 'cat-mask', name: '마스크/팩', parent: 'cat-skincare'});
CREATE (:Category {id: 'cat-cleanser', name: '클렌저', parent: 'cat-skincare'});
CREATE (:Category {id: 'cat-makeup', name: '메이크업', parent: ''});
CREATE (:Category {id: 'cat-lip', name: '립', parent: 'cat-makeup'});
CREATE (:Category {id: 'cat-foundation', name: '파운데이션/베이스', parent: 'cat-makeup'});
CREATE (:Category {id: 'cat-eye', name: '아이', parent: 'cat-makeup'});
CREATE (:Category {id: 'cat-haircare', name: '헤어케어', parent: ''});
CREATE (:Category {id: 'cat-shampoo', name: '샴푸/트리트먼트', parent: 'cat-haircare'});
CREATE (:Category {id: 'cat-bodycare', name: '바디케어', parent: ''});
CREATE (:Category {id: 'cat-bodywash', name: '바디워시/로션', parent: 'cat-bodycare'});
CREATE (:Category {id: 'cat-mens', name: '남성', parent: ''});

// --- Brands (30 brands) ---
CREATE (:Brand {id: 'br-roundlab', name: '라운드랩', country: '한국', founded_year: 2018});
CREATE (:Brand {id: 'br-cosrx', name: '코스알엑스', country: '한국', founded_year: 2013});
CREATE (:Brand {id: 'br-innisfree', name: '이니스프리', country: '한국', founded_year: 2000});
CREATE (:Brand {id: 'br-torriden', name: '토리든', country: '한국', founded_year: 2019});
CREATE (:Brand {id: 'br-anua', name: '아누아', country: '한국', founded_year: 2020});
CREATE (:Brand {id: 'br-beauty-of-joseon', name: '조선미녀', country: '한국', founded_year: 2017});
CREATE (:Brand {id: 'br-isntree', name: '이즈앤트리', country: '한국', founded_year: 2011});
CREATE (:Brand {id: 'br-mixsoon', name: '믹순', country: '한국', founded_year: 2020});
CREATE (:Brand {id: 'br-numbuzin', name: '넘버즈인', country: '한국', founded_year: 2020});
CREATE (:Brand {id: 'br-skin1004', name: '스킨1004', country: '한국', founded_year: 2017});
CREATE (:Brand {id: 'br-medicube', name: '메디큐브', country: '한국', founded_year: 2018});
CREATE (:Brand {id: 'br-romand', name: '롬앤', country: '한국', founded_year: 2016});
CREATE (:Brand {id: 'br-clio', name: '클리오', country: '한국', founded_year: 1993});
CREATE (:Brand {id: 'br-peripera', name: '페리페라', country: '한국', founded_year: 2005});
CREATE (:Brand {id: 'br-tirtir', name: '티르티르', country: '한국', founded_year: 2017});
CREATE (:Brand {id: 'br-amuse', name: '어뮤즈', country: '한국', founded_year: 2018});
CREATE (:Brand {id: 'br-laneige', name: '라네즈', country: '한국', founded_year: 1994});
CREATE (:Brand {id: 'br-sulwhasoo', name: '설화수', country: '한국', founded_year: 1997});
CREATE (:Brand {id: 'br-mise-en-scene', name: '미장센', country: '한국', founded_year: 2000});
CREATE (:Brand {id: 'br-ryo', name: '려', country: '한국', founded_year: 2008});
CREATE (:Brand {id: 'br-moremo', name: '모레모', country: '한국', founded_year: 2018});
CREATE (:Brand {id: 'br-kundal', name: '쿤달', country: '한국', founded_year: 2017});
CREATE (:Brand {id: 'br-illiyoon', name: '일리윤', country: '한국', founded_year: 2005});
CREATE (:Brand {id: 'br-dr-g', name: '닥터지', country: '한국', founded_year: 2003});
CREATE (:Brand {id: 'br-some-by-mi', name: '썸바이미', country: '한국', founded_year: 2016});
CREATE (:Brand {id: 'br-celimax', name: '셀리맥스', country: '한국', founded_year: 2018});
CREATE (:Brand {id: 'br-abib', name: '아비브', country: '한국', founded_year: 2017});
CREATE (:Brand {id: 'br-etude', name: '에뛰드', country: '한국', founded_year: 1985});
CREATE (:Brand {id: 'br-dashu', name: '다슈', country: '한국', founded_year: 2015});
CREATE (:Brand {id: 'br-benton', name: '벤톤', country: '한국', founded_year: 2011});
CREATE (:Brand {id: 'br-mediheal', name: '메디힐', country: '한국', founded_year: 2009});

// --- Products (100 products) ---

// Skincare - Toner (12)
CREATE (:Product {id: 'p001', name: '라운드랩 독도 토너', price: 13500, size: '200ml', launch_date: '2020-03-01'});
CREATE (:Product {id: 'p002', name: '아누아 어성초 77 토너', price: 18000, size: '250ml', launch_date: '2021-01-15'});
CREATE (:Product {id: 'p003', name: '이즈앤트리 히알루로닉 애시드 토너', price: 14900, size: '200ml', launch_date: '2019-06-01'});
CREATE (:Product {id: 'p004', name: '코스알엑스 AHA/BHA 클래리파잉 트리트먼트 토너', price: 13800, size: '150ml', launch_date: '2018-09-01'});
CREATE (:Product {id: 'p005', name: '토리든 다이브인 저분자 히알루론산 토너', price: 15800, size: '300ml', launch_date: '2022-03-01'});
CREATE (:Product {id: 'p006', name: '넘버즈인 1번 맑은 결 토너패드', price: 14500, size: '70매', launch_date: '2022-05-01'});
CREATE (:Product {id: 'p007', name: '라운드랩 소나무 진정 토너', price: 14500, size: '200ml', launch_date: '2021-08-01'});
CREATE (:Product {id: 'p008', name: '아비브 어성초 pH 밸런스 토너', price: 18000, size: '200ml', launch_date: '2021-04-01'});
CREATE (:Product {id: 'p009', name: '믹순 빈 에센스', price: 17000, size: '100ml', launch_date: '2022-01-01'});
CREATE (:Product {id: 'p010', name: '이니스프리 그린티 씨드 스킨', price: 16000, size: '200ml', launch_date: '2019-01-01'});
CREATE (:Product {id: 'p011', name: '벤톤 알로에 BHA 스킨 토너', price: 15000, size: '200ml', launch_date: '2020-07-01'});
CREATE (:Product {id: 'p012', name: '셀리맥스 더 리얼 노니 에너지 앰플 토너', price: 16500, size: '200ml', launch_date: '2022-02-01'});

// Skincare - Serum (15)
CREATE (:Product {id: 'p013', name: '코스알엑스 어드밴스드 스네일 96 뮤신 파워 에센스', price: 16800, size: '100ml', launch_date: '2017-01-01'});
CREATE (:Product {id: 'p014', name: '토리든 다이브인 저분자 히알루론산 세럼', price: 18500, size: '50ml', launch_date: '2021-09-01'});
CREATE (:Product {id: 'p015', name: '넘버즈인 5번 비타톤 글루타치온 C 세럼', price: 19800, size: '30ml', launch_date: '2022-06-01'});
CREATE (:Product {id: 'p016', name: '조선미녀 맑은 쌀 뷰티 세럼', price: 17000, size: '30ml', launch_date: '2022-02-01'});
CREATE (:Product {id: 'p017', name: '아누아 어성초 77 수딩 세럼', price: 20000, size: '30ml', launch_date: '2021-06-01'});
CREATE (:Product {id: 'p018', name: '이즈앤트리 C 비타민 세럼', price: 16000, size: '20ml', launch_date: '2021-03-01'});
CREATE (:Product {id: 'p019', name: '스킨1004 마다가스카르 센텔라 앰플', price: 18500, size: '55ml', launch_date: '2020-01-01'});
CREATE (:Product {id: 'p020', name: '믹순 갈락토미세스 발효 에센스', price: 18000, size: '100ml', launch_date: '2022-04-01'});
CREATE (:Product {id: 'p021', name: '메디큐브 콜라겐 나이트 래핑 마스크', price: 26000, size: '75ml', launch_date: '2023-01-01'});
CREATE (:Product {id: 'p022', name: '라운드랩 자작나무 수분 세럼', price: 15800, size: '30ml', launch_date: '2022-07-01'});
CREATE (:Product {id: 'p023', name: '코스알엑스 더 비타민C 23 세럼', price: 15800, size: '20ml', launch_date: '2020-05-01'});
CREATE (:Product {id: 'p024', name: '닥터지 레드 블레미쉬 클리어 수딩 크림', price: 22000, size: '70ml', launch_date: '2019-09-01'});
CREATE (:Product {id: 'p025', name: '썸바이미 유자 나이아신 브라이트닝 세럼', price: 16500, size: '30ml', launch_date: '2021-11-01'});
CREATE (:Product {id: 'p026', name: '이니스프리 레티놀 시카 리페어 세럼', price: 24000, size: '30ml', launch_date: '2023-03-01'});
CREATE (:Product {id: 'p027', name: '라네즈 워터 슬리핑 마스크 EX', price: 28000, size: '70ml', launch_date: '2022-09-01'});

// Skincare - Cream (10)
CREATE (:Product {id: 'p028', name: '라운드랩 독도 크림', price: 19500, size: '80ml', launch_date: '2020-09-01'});
CREATE (:Product {id: 'p029', name: '코스알엑스 어드밴스드 스네일 92 올인원 크림', price: 19800, size: '100ml', launch_date: '2017-05-01'});
CREATE (:Product {id: 'p030', name: '일리윤 세라마이드 아토 컨센트레이트 크림', price: 18900, size: '200ml', launch_date: '2020-10-01'});
CREATE (:Product {id: 'p031', name: '토리든 다이브인 수분 크림', price: 22000, size: '100ml', launch_date: '2022-08-01'});
CREATE (:Product {id: 'p032', name: '이즈앤트리 알로에 수딩 젤', price: 12500, size: '80ml', launch_date: '2019-05-01'});
CREATE (:Product {id: 'p033', name: '조선미녀 조선왕조 크림', price: 16000, size: '50ml', launch_date: '2021-01-01'});
CREATE (:Product {id: 'p034', name: '설화수 자음생 크림', price: 89000, size: '60ml', launch_date: '2021-06-01'});
CREATE (:Product {id: 'p035', name: '이니스프리 그린티 씨드 크림', price: 22000, size: '50ml', launch_date: '2020-02-01'});
CREATE (:Product {id: 'p036', name: '넘버즈인 3번 콜라겐 탄력 크림', price: 23800, size: '50ml', launch_date: '2023-02-01'});
CREATE (:Product {id: 'p037', name: '아비브 크림 코팅 마스크', price: 16500, size: '70ml', launch_date: '2022-04-01'});

// Skincare - Suncare (8)
CREATE (:Product {id: 'p038', name: '조선미녀 맑은 쌀 선크림', price: 15000, size: '50ml', launch_date: '2022-01-01'});
CREATE (:Product {id: 'p039', name: '라운드랩 자작나무 수분 선크림', price: 14500, size: '50ml', launch_date: '2021-05-01'});
CREATE (:Product {id: 'p040', name: '토리든 다이브인 워터리 선크림', price: 16800, size: '50ml', launch_date: '2023-04-01'});
CREATE (:Product {id: 'p041', name: '이즈앤트리 히알루로닉 애시드 워터리 선젤', price: 14900, size: '50ml', launch_date: '2022-05-01'});
CREATE (:Product {id: 'p042', name: '스킨1004 마다가스카르 센텔라 에어핏 선크림', price: 16500, size: '50ml', launch_date: '2022-06-01'});
CREATE (:Product {id: 'p043', name: '닥터지 그린 마일드 업 선 플러스', price: 19800, size: '50ml', launch_date: '2021-07-01'});
CREATE (:Product {id: 'p044', name: '셀리맥스 더 리얼 노니 선크림', price: 15000, size: '50ml', launch_date: '2023-01-01'});
CREATE (:Product {id: 'p045', name: '아비브 퀵 선스틱 프로텍션 바', price: 18500, size: '22g', launch_date: '2023-05-01'});

// Skincare - Mask (5)
CREATE (:Product {id: 'p046', name: '메디힐 N.M.F 아쿠아링 앰플 마스크', price: 15000, size: '10매', launch_date: '2018-01-01'});
CREATE (:Product {id: 'p047', name: '아비브 약산성 pH 시트 마스크', price: 14800, size: '10매', launch_date: '2020-06-01'});
CREATE (:Product {id: 'p048', name: '이니스프리 수퍼 화산송이 모공 마스크', price: 12000, size: '100ml', launch_date: '2019-08-01'});
CREATE (:Product {id: 'p049', name: '코스알엑스 아크네 패치', price: 5500, size: '24매', launch_date: '2019-01-01'});
CREATE (:Product {id: 'p050', name: '라운드랩 독도 머드팩', price: 13500, size: '100ml', launch_date: '2021-03-01'});

// Skincare - Cleanser (6)
CREATE (:Product {id: 'p051', name: '라운드랩 독도 클렌징 오일', price: 15000, size: '200ml', launch_date: '2021-01-01'});
CREATE (:Product {id: 'p052', name: '코스알엑스 로우 pH 굿모닝 젤 클렌저', price: 9800, size: '150ml', launch_date: '2016-09-01'});
CREATE (:Product {id: 'p053', name: '이니스프리 그린티 클렌징 폼', price: 8000, size: '150ml', launch_date: '2018-03-01'});
CREATE (:Product {id: 'p054', name: '아누아 어성초 클렌징 오일', price: 17000, size: '200ml', launch_date: '2022-09-01'});
CREATE (:Product {id: 'p055', name: '토리든 밀크 타입 클렌징 로션', price: 14800, size: '200ml', launch_date: '2023-06-01'});
CREATE (:Product {id: 'p056', name: '넘버즈인 2번 클렌징 폼', price: 12500, size: '120ml', launch_date: '2022-10-01'});

// Makeup - Lip (10)
CREATE (:Product {id: 'p057', name: '롬앤 쥬시 래스팅 틴트', price: 9900, size: '5.5g', launch_date: '2019-06-01'});
CREATE (:Product {id: 'p058', name: '페리페라 잉크 더 벨벳', price: 9800, size: '4g', launch_date: '2020-03-01'});
CREATE (:Product {id: 'p059', name: '클리오 멜팅 쉬어 립', price: 13000, size: '2.2g', launch_date: '2023-03-01'});
CREATE (:Product {id: 'p060', name: '어뮤즈 듀 틴트', price: 13500, size: '4g', launch_date: '2021-09-01'});
CREATE (:Product {id: 'p061', name: '롬앤 글래스팅 워터 틴트', price: 9900, size: '4g', launch_date: '2021-01-01'});
CREATE (:Product {id: 'p062', name: '에뛰드 픽싱 틴트', price: 10800, size: '4g', launch_date: '2022-03-01'});
CREATE (:Product {id: 'p063', name: '페리페라 잉크 무드 글로이 틴트', price: 10800, size: '4g', launch_date: '2023-01-01'});
CREATE (:Product {id: 'p064', name: '라네즈 립 글로이 밤', price: 18000, size: '10g', launch_date: '2023-06-01'});
CREATE (:Product {id: 'p065', name: '티르티르 마스크 핏 립 틴트', price: 13500, size: '4.5g', launch_date: '2023-04-01'});
CREATE (:Product {id: 'p066', name: '클리오 킬 래쉬 수퍼프루프 마스카라', price: 16000, size: '7g', launch_date: '2022-01-01'});

// Makeup - Foundation/Base (8)
CREATE (:Product {id: 'p067', name: '티르티르 마스크 핏 레드 쿠션', price: 26000, size: '18g', launch_date: '2022-08-01'});
CREATE (:Product {id: 'p068', name: '클리오 킬 커버 파운웨어 쿠션', price: 25000, size: '15g', launch_date: '2021-03-01'});
CREATE (:Product {id: 'p069', name: '라네즈 네오 쿠션 매트', price: 32000, size: '15g', launch_date: '2021-09-01'});
CREATE (:Product {id: 'p070', name: '이니스프리 노세범 미네랄 파우더', price: 9000, size: '5g', launch_date: '2017-06-01'});
CREATE (:Product {id: 'p071', name: '에뛰드 더블래스팅 세럼 파운데이션', price: 19800, size: '30ml', launch_date: '2023-09-01'});
CREATE (:Product {id: 'p072', name: '페리페라 잉크 브이 쉐이딩', price: 11800, size: '9.5g', launch_date: '2022-05-01'});
CREATE (:Product {id: 'p073', name: '어뮤즈 스킨 세럼 파운데이션', price: 28000, size: '30ml', launch_date: '2023-08-01'});
CREATE (:Product {id: 'p074', name: '메디큐브 에어 쿠션', price: 35000, size: '12g', launch_date: '2023-05-01'});

// Makeup - Eye (5)
CREATE (:Product {id: 'p075', name: '클리오 프로 아이 팔레트', price: 34000, size: '6g', launch_date: '2021-08-01'});
CREATE (:Product {id: 'p076', name: '롬앤 베러 댄 아이즈', price: 12000, size: '6.5g', launch_date: '2020-09-01'});
CREATE (:Product {id: 'p077', name: '페리페라 올 테이크 무드 팔레트', price: 18000, size: '8g', launch_date: '2023-02-01'});
CREATE (:Product {id: 'p078', name: '에뛰드 플레이 컬러 아이즈', price: 15000, size: '7.2g', launch_date: '2020-06-01'});
CREATE (:Product {id: 'p079', name: '어뮤즈 아이 비건 쉬어 팔레트', price: 24000, size: '9.8g', launch_date: '2022-11-01'});

// Haircare (9)
CREATE (:Product {id: 'p080', name: '미장센 퍼펙트 세럼 오리지날', price: 11800, size: '80ml', launch_date: '2015-01-01'});
CREATE (:Product {id: 'p081', name: '려 자양윤모 샴푸', price: 12900, size: '400ml', launch_date: '2018-06-01'});
CREATE (:Product {id: 'p082', name: '모레모 워터 트리트먼트 미라클 10', price: 14900, size: '200ml', launch_date: '2020-01-01'});
CREATE (:Product {id: 'p083', name: '쿤달 허니 앤 마카다미아 샴푸', price: 12900, size: '500ml', launch_date: '2019-03-01'});
CREATE (:Product {id: 'p084', name: '이니스프리 그린티 민트 프레시 샴푸', price: 11000, size: '300ml', launch_date: '2020-05-01'});
CREATE (:Product {id: 'p085', name: '미장센 스타일링 스프레이', price: 8900, size: '200ml', launch_date: '2019-01-01'});
CREATE (:Product {id: 'p086', name: '려 흑운 모근 영양 샴푸', price: 15900, size: '400ml', launch_date: '2021-09-01'});
CREATE (:Product {id: 'p087', name: '모레모 헤어 에센스', price: 16800, size: '120ml', launch_date: '2021-04-01'});
CREATE (:Product {id: 'p088', name: '쿤달 프로틴 헤어 앰플', price: 14500, size: '150ml', launch_date: '2022-02-01'});

// Bodycare (7)
CREATE (:Product {id: 'p089', name: '일리윤 세라마이드 아토 바디워시', price: 14500, size: '500ml', launch_date: '2020-08-01'});
CREATE (:Product {id: 'p090', name: '쿤달 퓨어 바디워시', price: 12500, size: '500ml', launch_date: '2019-07-01'});
CREATE (:Product {id: 'p091', name: '일리윤 세라마이드 아토 바디로션', price: 16900, size: '350ml', launch_date: '2020-08-01'});
CREATE (:Product {id: 'p092', name: '이니스프리 올리브 리얼 바디로션', price: 12000, size: '310ml', launch_date: '2018-09-01'});
CREATE (:Product {id: 'p093', name: '닥터지 인텐시브 바디 로션', price: 18500, size: '350ml', launch_date: '2021-10-01'});
CREATE (:Product {id: 'p094', name: '코스알엑스 AHA/BHA 비타민 바디 스프레이', price: 14800, size: '150ml', launch_date: '2022-03-01'});
CREATE (:Product {id: 'p095', name: '라운드랩 독도 바디미스트', price: 13500, size: '200ml', launch_date: '2022-11-01'});

// Men's (5)
CREATE (:Product {id: 'p096', name: '다슈 데일리 스칼프 샴푸', price: 15900, size: '500ml', launch_date: '2020-01-01'});
CREATE (:Product {id: 'p097', name: '다슈 포 맨 올인원 로션', price: 18900, size: '153ml', launch_date: '2021-05-01'});
CREATE (:Product {id: 'p098', name: '다슈 울트라 홀딩 왁스', price: 12000, size: '100ml', launch_date: '2019-09-01'});
CREATE (:Product {id: 'p099', name: '이니스프리 포레스트 포맨 올인원 에센스', price: 22000, size: '100ml', launch_date: '2020-04-01'});
CREATE (:Product {id: 'p100', name: '라운드랩 독도 맨즈 올인원 로션', price: 17500, size: '150ml', launch_date: '2023-02-01'});

// --- Stores (20) ---
CREATE (:Store {id: 'st-myeongdong', name: '올리브영 명동 타운', address: '서울 중구 명동8나길 31', store_type: 'Town', open_date: '2018-12-01'});
CREATE (:Store {id: 'st-gangnam', name: '올리브영 강남 타운', address: '서울 강남구 강남대로 426', store_type: 'Town', open_date: '2019-06-01'});
CREATE (:Store {id: 'st-hongdae', name: '올리브영 홍대 타운', address: '서울 마포구 양화로 160', store_type: 'Town', open_date: '2020-03-01'});
CREATE (:Store {id: 'st-jamsil', name: '올리브영 잠실 롯데월드몰', address: '서울 송파구 올림픽로 300', store_type: 'Regular', open_date: '2017-05-01'});
CREATE (:Store {id: 'st-sinchon', name: '올리브영 신촌점', address: '서울 서대문구 신촌로 83', store_type: 'Regular', open_date: '2016-09-01'});
CREATE (:Store {id: 'st-itaewon', name: '올리브영 이태원점', address: '서울 용산구 이태원로 177', store_type: 'Regular', open_date: '2019-01-01'});
CREATE (:Store {id: 'st-yeouido', name: '올리브영 여의도 IFC점', address: '서울 영등포구 국제금융로 10', store_type: 'Regular', open_date: '2018-04-01'});
CREATE (:Store {id: 'st-coex', name: '올리브영 삼성 코엑스점', address: '서울 강남구 영동대로 513', store_type: 'Regular', open_date: '2017-11-01'});
CREATE (:Store {id: 'st-suwon', name: '올리브영 수원역점', address: '경기 수원시 팔달구 덕영대로 923', store_type: 'Regular', open_date: '2018-02-01'});
CREATE (:Store {id: 'st-bundang', name: '올리브영 분당 서현점', address: '경기 성남시 분당구 서현로 170', store_type: 'Regular', open_date: '2017-08-01'});
CREATE (:Store {id: 'st-ilsan', name: '올리브영 일산 라페스타점', address: '경기 고양시 일산서구 중앙로 1305', store_type: 'Regular', open_date: '2018-06-01'});
CREATE (:Store {id: 'st-hanam', name: '올리브영 하남 스타필드', address: '경기 하남시 미사대로 750', store_type: 'N', open_date: '2022-01-01'});
CREATE (:Store {id: 'st-incheon', name: '올리브영 인천 부평점', address: '인천 부평구 부평대로 35', store_type: 'Regular', open_date: '2017-03-01'});
CREATE (:Store {id: 'st-busan-seomyeon', name: '올리브영 서면 타운', address: '부산 부산진구 서면로 68번길 33', store_type: 'Town', open_date: '2019-09-01'});
CREATE (:Store {id: 'st-busan-nampo', name: '올리브영 남포점', address: '부산 중구 광복로 62', store_type: 'Regular', open_date: '2018-01-01'});
CREATE (:Store {id: 'st-busan-haeundae', name: '올리브영 해운대점', address: '부산 해운대구 해운대로 788', store_type: 'Regular', open_date: '2019-04-01'});
CREATE (:Store {id: 'st-daegu', name: '올리브영 대구 동성로점', address: '대구 중구 동성로2가 149', store_type: 'Regular', open_date: '2017-06-01'});
CREATE (:Store {id: 'st-daejeon', name: '올리브영 대전 둔산점', address: '대전 서구 대덕대로 211', store_type: 'Regular', open_date: '2018-08-01'});
CREATE (:Store {id: 'st-gwangju', name: '올리브영 광주 충장로점', address: '광주 동구 충장로 54', store_type: 'Regular', open_date: '2019-02-01'});
CREATE (:Store {id: 'st-dongdaemun', name: '올리브영 동대문 N점', address: '서울 중구 장충단로 253', store_type: 'N', open_date: '2023-03-01'});

// --- Store → Region ---
MATCH (s:Store {id: 'st-myeongdong'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-gangnam'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-hongdae'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-jamsil'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-sinchon'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-itaewon'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-yeouido'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-coex'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-dongdaemun'}), (r:Region {id: 'reg-seoul'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-suwon'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-bundang'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-ilsan'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-hanam'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-incheon'}), (r:Region {id: 'reg-incheon'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-busan-seomyeon'}), (r:Region {id: 'reg-busan'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-busan-nampo'}), (r:Region {id: 'reg-busan'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-busan-haeundae'}), (r:Region {id: 'reg-busan'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-daegu'}), (r:Region {id: 'reg-daegu'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-daejeon'}), (r:Region {id: 'reg-daejeon'}) CREATE (s)-[:LOCATED_IN]->(r);
MATCH (s:Store {id: 'st-gwangju'}), (r:Region {id: 'reg-gwangju'}) CREATE (s)-[:LOCATED_IN]->(r);

// --- Product → Brand ---
// 라운드랩
MATCH (p:Product {id: 'p001'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p007'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p022'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p028'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p039'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p050'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p051'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p095'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p100'}), (b:Brand {id: 'br-roundlab'}) CREATE (p)-[:MADE_BY]->(b);
// 코스알엑스
MATCH (p:Product {id: 'p004'}), (b:Brand {id: 'br-cosrx'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p013'}), (b:Brand {id: 'br-cosrx'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p023'}), (b:Brand {id: 'br-cosrx'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p029'}), (b:Brand {id: 'br-cosrx'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p049'}), (b:Brand {id: 'br-cosrx'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p052'}), (b:Brand {id: 'br-cosrx'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p094'}), (b:Brand {id: 'br-cosrx'}) CREATE (p)-[:MADE_BY]->(b);
// 이니스프리
MATCH (p:Product {id: 'p010'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p026'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p035'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p048'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p053'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p070'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p084'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p092'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p099'}), (b:Brand {id: 'br-innisfree'}) CREATE (p)-[:MADE_BY]->(b);
// 토리든
MATCH (p:Product {id: 'p005'}), (b:Brand {id: 'br-torriden'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p014'}), (b:Brand {id: 'br-torriden'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p031'}), (b:Brand {id: 'br-torriden'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p040'}), (b:Brand {id: 'br-torriden'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p055'}), (b:Brand {id: 'br-torriden'}) CREATE (p)-[:MADE_BY]->(b);
// 아누아
MATCH (p:Product {id: 'p002'}), (b:Brand {id: 'br-anua'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p017'}), (b:Brand {id: 'br-anua'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p054'}), (b:Brand {id: 'br-anua'}) CREATE (p)-[:MADE_BY]->(b);
// 조선미녀
MATCH (p:Product {id: 'p016'}), (b:Brand {id: 'br-beauty-of-joseon'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p033'}), (b:Brand {id: 'br-beauty-of-joseon'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p038'}), (b:Brand {id: 'br-beauty-of-joseon'}) CREATE (p)-[:MADE_BY]->(b);
// 이즈앤트리
MATCH (p:Product {id: 'p003'}), (b:Brand {id: 'br-isntree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p018'}), (b:Brand {id: 'br-isntree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p032'}), (b:Brand {id: 'br-isntree'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p041'}), (b:Brand {id: 'br-isntree'}) CREATE (p)-[:MADE_BY]->(b);
// 믹순
MATCH (p:Product {id: 'p009'}), (b:Brand {id: 'br-mixsoon'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p020'}), (b:Brand {id: 'br-mixsoon'}) CREATE (p)-[:MADE_BY]->(b);
// 넘버즈인
MATCH (p:Product {id: 'p006'}), (b:Brand {id: 'br-numbuzin'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p015'}), (b:Brand {id: 'br-numbuzin'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p036'}), (b:Brand {id: 'br-numbuzin'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p056'}), (b:Brand {id: 'br-numbuzin'}) CREATE (p)-[:MADE_BY]->(b);
// 스킨1004
MATCH (p:Product {id: 'p019'}), (b:Brand {id: 'br-skin1004'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p042'}), (b:Brand {id: 'br-skin1004'}) CREATE (p)-[:MADE_BY]->(b);
// 메디큐브
MATCH (p:Product {id: 'p021'}), (b:Brand {id: 'br-medicube'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p074'}), (b:Brand {id: 'br-medicube'}) CREATE (p)-[:MADE_BY]->(b);
// 롬앤
MATCH (p:Product {id: 'p057'}), (b:Brand {id: 'br-romand'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p061'}), (b:Brand {id: 'br-romand'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p076'}), (b:Brand {id: 'br-romand'}) CREATE (p)-[:MADE_BY]->(b);
// 클리오
MATCH (p:Product {id: 'p059'}), (b:Brand {id: 'br-clio'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p066'}), (b:Brand {id: 'br-clio'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p068'}), (b:Brand {id: 'br-clio'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p075'}), (b:Brand {id: 'br-clio'}) CREATE (p)-[:MADE_BY]->(b);
// 페리페라
MATCH (p:Product {id: 'p058'}), (b:Brand {id: 'br-peripera'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p063'}), (b:Brand {id: 'br-peripera'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p072'}), (b:Brand {id: 'br-peripera'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p077'}), (b:Brand {id: 'br-peripera'}) CREATE (p)-[:MADE_BY]->(b);
// 티르티르
MATCH (p:Product {id: 'p065'}), (b:Brand {id: 'br-tirtir'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p067'}), (b:Brand {id: 'br-tirtir'}) CREATE (p)-[:MADE_BY]->(b);
// 어뮤즈
MATCH (p:Product {id: 'p060'}), (b:Brand {id: 'br-amuse'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p073'}), (b:Brand {id: 'br-amuse'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p079'}), (b:Brand {id: 'br-amuse'}) CREATE (p)-[:MADE_BY]->(b);
// 라네즈
MATCH (p:Product {id: 'p027'}), (b:Brand {id: 'br-laneige'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p064'}), (b:Brand {id: 'br-laneige'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p069'}), (b:Brand {id: 'br-laneige'}) CREATE (p)-[:MADE_BY]->(b);
// 설화수
MATCH (p:Product {id: 'p034'}), (b:Brand {id: 'br-sulwhasoo'}) CREATE (p)-[:MADE_BY]->(b);
// 미장센
MATCH (p:Product {id: 'p080'}), (b:Brand {id: 'br-mise-en-scene'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p085'}), (b:Brand {id: 'br-mise-en-scene'}) CREATE (p)-[:MADE_BY]->(b);
// 려
MATCH (p:Product {id: 'p081'}), (b:Brand {id: 'br-ryo'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p086'}), (b:Brand {id: 'br-ryo'}) CREATE (p)-[:MADE_BY]->(b);
// 모레모
MATCH (p:Product {id: 'p082'}), (b:Brand {id: 'br-moremo'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p087'}), (b:Brand {id: 'br-moremo'}) CREATE (p)-[:MADE_BY]->(b);
// 쿤달
MATCH (p:Product {id: 'p083'}), (b:Brand {id: 'br-kundal'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p088'}), (b:Brand {id: 'br-kundal'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p090'}), (b:Brand {id: 'br-kundal'}) CREATE (p)-[:MADE_BY]->(b);
// 일리윤
MATCH (p:Product {id: 'p030'}), (b:Brand {id: 'br-illiyoon'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p089'}), (b:Brand {id: 'br-illiyoon'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p091'}), (b:Brand {id: 'br-illiyoon'}) CREATE (p)-[:MADE_BY]->(b);
// 닥터지
MATCH (p:Product {id: 'p024'}), (b:Brand {id: 'br-dr-g'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p043'}), (b:Brand {id: 'br-dr-g'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p093'}), (b:Brand {id: 'br-dr-g'}) CREATE (p)-[:MADE_BY]->(b);
// 썸바이미
MATCH (p:Product {id: 'p025'}), (b:Brand {id: 'br-some-by-mi'}) CREATE (p)-[:MADE_BY]->(b);
// 셀리맥스
MATCH (p:Product {id: 'p012'}), (b:Brand {id: 'br-celimax'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p044'}), (b:Brand {id: 'br-celimax'}) CREATE (p)-[:MADE_BY]->(b);
// 아비브
MATCH (p:Product {id: 'p008'}), (b:Brand {id: 'br-abib'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p037'}), (b:Brand {id: 'br-abib'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p045'}), (b:Brand {id: 'br-abib'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p047'}), (b:Brand {id: 'br-abib'}) CREATE (p)-[:MADE_BY]->(b);
// 에뛰드
MATCH (p:Product {id: 'p062'}), (b:Brand {id: 'br-etude'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p071'}), (b:Brand {id: 'br-etude'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p078'}), (b:Brand {id: 'br-etude'}) CREATE (p)-[:MADE_BY]->(b);
// 다슈
MATCH (p:Product {id: 'p096'}), (b:Brand {id: 'br-dashu'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p097'}), (b:Brand {id: 'br-dashu'}) CREATE (p)-[:MADE_BY]->(b);
MATCH (p:Product {id: 'p098'}), (b:Brand {id: 'br-dashu'}) CREATE (p)-[:MADE_BY]->(b);
// 벤톤
MATCH (p:Product {id: 'p011'}), (b:Brand {id: 'br-benton'}) CREATE (p)-[:MADE_BY]->(b);
// 메디힐
MATCH (p:Product {id: 'p046'}), (b:Brand {id: 'br-mediheal'}) CREATE (p)-[:MADE_BY]->(b);

// --- Product → Category ---
// Toner
MATCH (p:Product), (c:Category {id: 'cat-toner'}) WHERE p.id IN ['p001','p002','p003','p004','p005','p006','p007','p008','p009','p010','p011','p012'] CREATE (p)-[:IN_CATEGORY]->(c);
// Serum
MATCH (p:Product), (c:Category {id: 'cat-serum'}) WHERE p.id IN ['p013','p014','p015','p016','p017','p018','p019','p020','p021','p022','p023','p024','p025','p026','p027'] CREATE (p)-[:IN_CATEGORY]->(c);
// Cream
MATCH (p:Product), (c:Category {id: 'cat-cream'}) WHERE p.id IN ['p028','p029','p030','p031','p032','p033','p034','p035','p036','p037'] CREATE (p)-[:IN_CATEGORY]->(c);
// Suncare
MATCH (p:Product), (c:Category {id: 'cat-suncare'}) WHERE p.id IN ['p038','p039','p040','p041','p042','p043','p044','p045'] CREATE (p)-[:IN_CATEGORY]->(c);
// Mask
MATCH (p:Product), (c:Category {id: 'cat-mask'}) WHERE p.id IN ['p046','p047','p048','p049','p050'] CREATE (p)-[:IN_CATEGORY]->(c);
// Cleanser
MATCH (p:Product), (c:Category {id: 'cat-cleanser'}) WHERE p.id IN ['p051','p052','p053','p054','p055','p056'] CREATE (p)-[:IN_CATEGORY]->(c);
// Lip
MATCH (p:Product), (c:Category {id: 'cat-lip'}) WHERE p.id IN ['p057','p058','p059','p060','p061','p062','p063','p064','p065'] CREATE (p)-[:IN_CATEGORY]->(c);
// Foundation/Base
MATCH (p:Product), (c:Category {id: 'cat-foundation'}) WHERE p.id IN ['p067','p068','p069','p070','p071','p072','p073','p074'] CREATE (p)-[:IN_CATEGORY]->(c);
// Eye
MATCH (p:Product), (c:Category {id: 'cat-eye'}) WHERE p.id IN ['p066','p075','p076','p077','p078','p079'] CREATE (p)-[:IN_CATEGORY]->(c);
// Shampoo/Treatment
MATCH (p:Product), (c:Category {id: 'cat-shampoo'}) WHERE p.id IN ['p080','p081','p082','p083','p084','p085','p086','p087','p088'] CREATE (p)-[:IN_CATEGORY]->(c);
// Bodywash/Lotion
MATCH (p:Product), (c:Category {id: 'cat-bodywash'}) WHERE p.id IN ['p089','p090','p091','p092','p093','p094','p095'] CREATE (p)-[:IN_CATEGORY]->(c);
// Men's
MATCH (p:Product), (c:Category {id: 'cat-mens'}) WHERE p.id IN ['p096','p097','p098','p099','p100'] CREATE (p)-[:IN_CATEGORY]->(c);

// --- Customers (50) ---
CREATE (:Customer {id: 'cu-001', name: '김지현', age: 24, gender: 'F', membership_tier: 'Gold', join_date: '2022-03-15'});
CREATE (:Customer {id: 'cu-002', name: '이수연', age: 28, gender: 'F', membership_tier: 'Pink', join_date: '2021-06-10'});
CREATE (:Customer {id: 'cu-003', name: '박은지', age: 31, gender: 'F', membership_tier: 'Gold', join_date: '2020-11-22'});
CREATE (:Customer {id: 'cu-004', name: '정민아', age: 22, gender: 'F', membership_tier: 'Green', join_date: '2023-01-08'});
CREATE (:Customer {id: 'cu-005', name: '최하늘', age: 26, gender: 'F', membership_tier: 'Gold', join_date: '2021-09-05'});
CREATE (:Customer {id: 'cu-006', name: '강예림', age: 35, gender: 'F', membership_tier: 'Pink', join_date: '2019-04-18'});
CREATE (:Customer {id: 'cu-007', name: '조유진', age: 29, gender: 'F', membership_tier: 'Gold', join_date: '2020-07-30'});
CREATE (:Customer {id: 'cu-008', name: '윤서현', age: 20, gender: 'F', membership_tier: 'Green', join_date: '2023-05-12'});
CREATE (:Customer {id: 'cu-009', name: '한나영', age: 33, gender: 'F', membership_tier: 'Pink', join_date: '2019-12-01'});
CREATE (:Customer {id: 'cu-010', name: '오지은', age: 27, gender: 'F', membership_tier: 'Gold', join_date: '2021-02-14'});
CREATE (:Customer {id: 'cu-011', name: '김태현', age: 25, gender: 'M', membership_tier: 'Green', join_date: '2022-08-20'});
CREATE (:Customer {id: 'cu-012', name: '이준서', age: 30, gender: 'M', membership_tier: 'Gold', join_date: '2021-01-11'});
CREATE (:Customer {id: 'cu-013', name: '박서준', age: 34, gender: 'M', membership_tier: 'Green', join_date: '2022-04-03'});
CREATE (:Customer {id: 'cu-014', name: '정우진', age: 23, gender: 'M', membership_tier: 'Green', join_date: '2023-02-28'});
CREATE (:Customer {id: 'cu-015', name: '송민지', age: 26, gender: 'F', membership_tier: 'Gold', join_date: '2021-05-16'});
CREATE (:Customer {id: 'cu-016', name: '임채원', age: 21, gender: 'F', membership_tier: 'Green', join_date: '2023-07-09'});
CREATE (:Customer {id: 'cu-017', name: '장수아', age: 38, gender: 'F', membership_tier: 'Pink', join_date: '2018-11-25'});
CREATE (:Customer {id: 'cu-018', name: '백지영', age: 29, gender: 'F', membership_tier: 'Gold', join_date: '2020-09-14'});
CREATE (:Customer {id: 'cu-019', name: '고은서', age: 24, gender: 'F', membership_tier: 'Green', join_date: '2022-12-01'});
CREATE (:Customer {id: 'cu-020', name: '유하린', age: 32, gender: 'F', membership_tier: 'Pink', join_date: '2019-08-07'});
CREATE (:Customer {id: 'cu-021', name: '신예은', age: 27, gender: 'F', membership_tier: 'Gold', join_date: '2021-03-22'});
CREATE (:Customer {id: 'cu-022', name: '권지수', age: 25, gender: 'F', membership_tier: 'Green', join_date: '2022-06-15'});
CREATE (:Customer {id: 'cu-023', name: '문소희', age: 30, gender: 'F', membership_tier: 'Gold', join_date: '2020-04-10'});
CREATE (:Customer {id: 'cu-024', name: '황미래', age: 36, gender: 'F', membership_tier: 'Pink', join_date: '2019-01-20'});
CREATE (:Customer {id: 'cu-025', name: '남도윤', age: 28, gender: 'M', membership_tier: 'Gold', join_date: '2021-07-04'});
CREATE (:Customer {id: 'cu-026', name: '안서윤', age: 23, gender: 'F', membership_tier: 'Green', join_date: '2023-03-18'});
CREATE (:Customer {id: 'cu-027', name: '전가영', age: 31, gender: 'F', membership_tier: 'Gold', join_date: '2020-10-29'});
CREATE (:Customer {id: 'cu-028', name: '양지호', age: 26, gender: 'M', membership_tier: 'Green', join_date: '2022-09-01'});
CREATE (:Customer {id: 'cu-029', name: '김다연', age: 22, gender: 'F', membership_tier: 'Green', join_date: '2023-04-25'});
CREATE (:Customer {id: 'cu-030', name: '이채린', age: 34, gender: 'F', membership_tier: 'Pink', join_date: '2019-06-13'});
CREATE (:Customer {id: 'cu-031', name: '박시우', age: 27, gender: 'M', membership_tier: 'Green', join_date: '2022-01-07'});
CREATE (:Customer {id: 'cu-032', name: '정유나', age: 25, gender: 'F', membership_tier: 'Gold', join_date: '2021-11-19'});
CREATE (:Customer {id: 'cu-033', name: '홍서진', age: 29, gender: 'F', membership_tier: 'Gold', join_date: '2020-08-03'});
CREATE (:Customer {id: 'cu-034', name: '노현우', age: 32, gender: 'M', membership_tier: 'Gold', join_date: '2021-04-27'});
CREATE (:Customer {id: 'cu-035', name: '추민서', age: 21, gender: 'F', membership_tier: 'Green', join_date: '2023-06-30'});
CREATE (:Customer {id: 'cu-036', name: '배소율', age: 28, gender: 'F', membership_tier: 'Gold', join_date: '2021-08-12'});
CREATE (:Customer {id: 'cu-037', name: '성재민', age: 24, gender: 'M', membership_tier: 'Green', join_date: '2022-11-05'});
CREATE (:Customer {id: 'cu-038', name: '류지안', age: 33, gender: 'F', membership_tier: 'Pink', join_date: '2019-10-16'});
CREATE (:Customer {id: 'cu-039', name: '심하윤', age: 26, gender: 'F', membership_tier: 'Gold', join_date: '2021-12-24'});
CREATE (:Customer {id: 'cu-040', name: '하은비', age: 30, gender: 'F', membership_tier: 'Gold', join_date: '2020-05-08'});
CREATE (:Customer {id: 'cu-041', name: '구본선', age: 37, gender: 'M', membership_tier: 'Gold', join_date: '2020-02-14'});
CREATE (:Customer {id: 'cu-042', name: '서다영', age: 23, gender: 'F', membership_tier: 'Green', join_date: '2023-01-31'});
CREATE (:Customer {id: 'cu-043', name: '차은우', age: 29, gender: 'M', membership_tier: 'Green', join_date: '2022-07-22'});
CREATE (:Customer {id: 'cu-044', name: '봉수진', age: 25, gender: 'F', membership_tier: 'Gold', join_date: '2021-10-09'});
CREATE (:Customer {id: 'cu-045', name: '도경수', age: 31, gender: 'M', membership_tier: 'Green', join_date: '2022-05-17'});
CREATE (:Customer {id: 'cu-046', name: '탁지원', age: 27, gender: 'F', membership_tier: 'Gold', join_date: '2021-06-28'});
CREATE (:Customer {id: 'cu-047', name: '방소연', age: 20, gender: 'F', membership_tier: 'Green', join_date: '2023-08-14'});
CREATE (:Customer {id: 'cu-048', name: '표현정', age: 35, gender: 'F', membership_tier: 'Pink', join_date: '2019-03-22'});
CREATE (:Customer {id: 'cu-049', name: '엄지혁', age: 28, gender: 'M', membership_tier: 'Green', join_date: '2022-10-11'});
CREATE (:Customer {id: 'cu-050', name: '피수빈', age: 24, gender: 'F', membership_tier: 'Green', join_date: '2023-02-06'});

// --- Customer → Region ---
MATCH (cu:Customer {id: 'cu-001'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-002'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-003'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-004'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-005'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-006'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-007'}), (r:Region {id: 'reg-busan'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-008'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-009'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-010'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-011'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-012'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-013'}), (r:Region {id: 'reg-busan'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-014'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-015'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-016'}), (r:Region {id: 'reg-daegu'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-017'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-018'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-019'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-020'}), (r:Region {id: 'reg-busan'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-021'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-022'}), (r:Region {id: 'reg-incheon'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-023'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-024'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-025'}), (r:Region {id: 'reg-daejeon'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-026'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-027'}), (r:Region {id: 'reg-gwangju'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-028'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-029'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-030'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-031'}), (r:Region {id: 'reg-busan'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-032'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-033'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-034'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-035'}), (r:Region {id: 'reg-daegu'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-036'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-037'}), (r:Region {id: 'reg-incheon'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-038'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-039'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-040'}), (r:Region {id: 'reg-busan'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-041'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-042'}), (r:Region {id: 'reg-gwangju'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-043'}), (r:Region {id: 'reg-daejeon'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-044'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-045'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-046'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-047'}), (r:Region {id: 'reg-seoul'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-048'}), (r:Region {id: 'reg-gyeonggi'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-049'}), (r:Region {id: 'reg-busan'}) CREATE (cu)-[:MEMBER_OF]->(r);
MATCH (cu:Customer {id: 'cu-050'}), (r:Region {id: 'reg-daegu'}) CREATE (cu)-[:MEMBER_OF]->(r);

// --- Promotions (5) ---
CREATE (:Promotion {id: 'pm-001', name: '봄맞이 스킨케어 세일', discount_pct: 20, start_date: '2025-03-01', end_date: '2025-03-31'});
CREATE (:Promotion {id: 'pm-002', name: '여름 선케어 특가', discount_pct: 15, start_date: '2025-06-01', end_date: '2025-07-31'});
CREATE (:Promotion {id: 'pm-003', name: '올영세일 (메가할인)', discount_pct: 30, start_date: '2025-09-15', end_date: '2025-09-30'});
CREATE (:Promotion {id: 'pm-004', name: '블랙프라이데이 특가', discount_pct: 25, start_date: '2025-11-20', end_date: '2025-11-30'});
CREATE (:Promotion {id: 'pm-005', name: '신상 런칭 기념 할인', discount_pct: 10, start_date: '2025-04-01', end_date: '2025-04-15'});

// Promotion → Product
MATCH (pm:Promotion {id: 'pm-001'}), (p:Product) WHERE p.id IN ['p001','p002','p005','p013','p014','p028','p029','p030'] CREATE (pm)-[:APPLIES_TO]->(p);
MATCH (pm:Promotion {id: 'pm-002'}), (p:Product) WHERE p.id IN ['p038','p039','p040','p041','p042','p043','p044','p045'] CREATE (pm)-[:APPLIES_TO]->(p);
MATCH (pm:Promotion {id: 'pm-003'}), (p:Product) WHERE p.id IN ['p057','p058','p060','p067','p068','p075','p080','p089'] CREATE (pm)-[:APPLIES_TO]->(p);
MATCH (pm:Promotion {id: 'pm-004'}), (p:Product) WHERE p.id IN ['p034','p027','p069','p074','p026','p021'] CREATE (pm)-[:APPLIES_TO]->(p);
MATCH (pm:Promotion {id: 'pm-005'}), (p:Product) WHERE p.id IN ['p040','p055','p065','p073','p074'] CREATE (pm)-[:APPLIES_TO]->(p);

// --- Brand → Store (SUPPLIES_TO) — major brands supply to most stores ---
MATCH (b:Brand {id: 'br-roundlab'}), (s:Store) WHERE s.id IN ['st-myeongdong','st-gangnam','st-hongdae','st-jamsil','st-suwon','st-busan-seomyeon','st-daegu','st-hanam'] CREATE (b)-[:SUPPLIES_TO]->(s);
MATCH (b:Brand {id: 'br-cosrx'}), (s:Store) WHERE s.id IN ['st-myeongdong','st-gangnam','st-hongdae','st-sinchon','st-bundang','st-busan-nampo','st-daejeon','st-gwangju'] CREATE (b)-[:SUPPLIES_TO]->(s);
MATCH (b:Brand {id: 'br-innisfree'}), (s:Store) CREATE (b)-[:SUPPLIES_TO]->(s);
MATCH (b:Brand {id: 'br-romand'}), (s:Store) WHERE s.id IN ['st-myeongdong','st-gangnam','st-hongdae','st-jamsil','st-coex','st-busan-seomyeon','st-hanam','st-dongdaemun'] CREATE (b)-[:SUPPLIES_TO]->(s);
MATCH (b:Brand {id: 'br-laneige'}), (s:Store) CREATE (b)-[:SUPPLIES_TO]->(s);
MATCH (b:Brand {id: 'br-torriden'}), (s:Store) WHERE s.id IN ['st-myeongdong','st-gangnam','st-hongdae','st-suwon','st-ilsan','st-busan-haeundae','st-dongdaemun'] CREATE (b)-[:SUPPLIES_TO]->(s);

// --- Transactions (200) + CONTAINS edges ---
// Using UNWIND for batch creation

// Batch 1: High-traffic store (명동) — 40 transactions
UNWIND range(1, 40) AS i
WITH i,
  CASE i % 5 WHEN 0 THEN 'card' WHEN 1 THEN 'card' WHEN 2 THEN 'kakao_pay' WHEN 3 THEN 'naver_pay' ELSE 'cash' END AS pay,
  CASE i % 12 WHEN 0 THEN 1 WHEN 1 THEN 1 WHEN 2 THEN 2 WHEN 3 THEN 2 WHEN 4 THEN 3 WHEN 5 THEN 3 WHEN 6 THEN 4 WHEN 7 THEN 5 WHEN 8 THEN 6 WHEN 9 THEN 7 WHEN 10 THEN 8 WHEN 11 THEN 9 END AS month
CREATE (t:Transaction {
  id: 'tx-' + toString(1000 + i),
  total_amount: 15000 + (i * 1300) % 85000,
  payment_method: pay,
  purchased_at: '2025-' + right('0' + toString(month + 1), 2) + '-' + right('0' + toString((i % 28) + 1), 2) + 'T' + right('0' + toString(10 + i % 12), 2) + ':' + right('0' + toString(i % 60), 2) + ':00'
})
WITH t, i
MATCH (s:Store {id: 'st-myeongdong'})
CREATE (t)-[:AT_STORE]->(s)
WITH t, i
MATCH (cu:Customer {id: 'cu-' + right('000' + toString((i % 50) + 1), 3)})
CREATE (cu)-[:PURCHASED]->(t)
WITH t, i
MATCH (p:Product {id: 'p' + right('000' + toString((i % 100) + 1), 3)})
CREATE (t)-[:CONTAINS {quantity: 1 + i % 3, unit_price: p.price}]->(p);

// Batch 2: 강남 — 30 transactions
UNWIND range(1, 30) AS i
WITH i,
  CASE i % 4 WHEN 0 THEN 'card' WHEN 1 THEN 'kakao_pay' WHEN 2 THEN 'naver_pay' ELSE 'card' END AS pay,
  CASE i % 12 WHEN 0 THEN 1 WHEN 1 THEN 2 WHEN 2 THEN 3 WHEN 3 THEN 4 WHEN 4 THEN 5 WHEN 5 THEN 6 WHEN 6 THEN 7 WHEN 7 THEN 8 WHEN 8 THEN 9 WHEN 9 THEN 10 WHEN 10 THEN 11 WHEN 11 THEN 12 END AS month
CREATE (t:Transaction {
  id: 'tx-' + toString(1100 + i),
  total_amount: 20000 + (i * 1700) % 90000,
  payment_method: pay,
  purchased_at: '2025-' + right('0' + toString(month), 2) + '-' + right('0' + toString((i % 28) + 1), 2) + 'T' + right('0' + toString(11 + i % 10), 2) + ':' + right('0' + toString(i * 7 % 60), 2) + ':00'
})
WITH t, i
MATCH (s:Store {id: 'st-gangnam'})
CREATE (t)-[:AT_STORE]->(s)
WITH t, i
MATCH (cu:Customer {id: 'cu-' + right('000' + toString((i % 30) + 1), 3)})
CREATE (cu)-[:PURCHASED]->(t)
WITH t, i
MATCH (p:Product {id: 'p' + right('000' + toString(((i * 3) % 100) + 1), 3)})
CREATE (t)-[:CONTAINS {quantity: 1 + i % 2, unit_price: p.price}]->(p);

// Batch 3: 홍대 — 25 transactions
UNWIND range(1, 25) AS i
WITH i,
  CASE i % 3 WHEN 0 THEN 'card' WHEN 1 THEN 'kakao_pay' ELSE 'naver_pay' END AS pay
CREATE (t:Transaction {
  id: 'tx-' + toString(1200 + i),
  total_amount: 12000 + (i * 2100) % 70000,
  payment_method: pay,
  purchased_at: '2025-' + right('0' + toString((i % 12) + 1), 2) + '-' + right('0' + toString((i % 28) + 1), 2) + 'T14:' + right('0' + toString(i * 3 % 60), 2) + ':00'
})
WITH t, i
MATCH (s:Store {id: 'st-hongdae'})
CREATE (t)-[:AT_STORE]->(s)
WITH t, i
MATCH (cu:Customer {id: 'cu-' + right('000' + toString((i % 40) + 1), 3)})
CREATE (cu)-[:PURCHASED]->(t)
WITH t, i
MATCH (p:Product {id: 'p' + right('000' + toString(((i * 7) % 100) + 1), 3)})
CREATE (t)-[:CONTAINS {quantity: 1, unit_price: p.price}]->(p);

// Batch 4: 서면(부산) — 20 transactions
UNWIND range(1, 20) AS i
WITH i,
  CASE i % 3 WHEN 0 THEN 'card' WHEN 1 THEN 'kakao_pay' ELSE 'cash' END AS pay
CREATE (t:Transaction {
  id: 'tx-' + toString(1300 + i),
  total_amount: 10000 + (i * 1500) % 60000,
  payment_method: pay,
  purchased_at: '2025-' + right('0' + toString((i % 12) + 1), 2) + '-' + right('0' + toString((i * 3 % 28) + 1), 2) + 'T16:' + right('0' + toString(i * 5 % 60), 2) + ':00'
})
WITH t, i
MATCH (s:Store {id: 'st-busan-seomyeon'})
CREATE (t)-[:AT_STORE]->(s)
WITH t, i
MATCH (cu:Customer {id: 'cu-' + right('000' + toString(((i + 5) % 50) + 1), 3)})
CREATE (cu)-[:PURCHASED]->(t)
WITH t, i
MATCH (p:Product {id: 'p' + right('000' + toString(((i * 11) % 100) + 1), 3)})
CREATE (t)-[:CONTAINS {quantity: 1 + i % 2, unit_price: p.price}]->(p);

// Batch 5: Other stores — 85 transactions spread across remaining stores
UNWIND range(1, 85) AS i
WITH i,
  CASE i % 16
    WHEN 0 THEN 'st-jamsil' WHEN 1 THEN 'st-sinchon' WHEN 2 THEN 'st-itaewon'
    WHEN 3 THEN 'st-yeouido' WHEN 4 THEN 'st-coex' WHEN 5 THEN 'st-suwon'
    WHEN 6 THEN 'st-bundang' WHEN 7 THEN 'st-ilsan' WHEN 8 THEN 'st-hanam'
    WHEN 9 THEN 'st-incheon' WHEN 10 THEN 'st-busan-nampo' WHEN 11 THEN 'st-busan-haeundae'
    WHEN 12 THEN 'st-daegu' WHEN 13 THEN 'st-daejeon' WHEN 14 THEN 'st-gwangju'
    ELSE 'st-dongdaemun'
  END AS store_id,
  CASE i % 4 WHEN 0 THEN 'card' WHEN 1 THEN 'kakao_pay' WHEN 2 THEN 'naver_pay' ELSE 'card' END AS pay
CREATE (t:Transaction {
  id: 'tx-' + toString(1400 + i),
  total_amount: 8000 + (i * 1900) % 95000,
  payment_method: pay,
  purchased_at: '2025-' + right('0' + toString((i % 12) + 1), 2) + '-' + right('0' + toString((i % 28) + 1), 2) + 'T' + right('0' + toString(9 + i % 13), 2) + ':' + right('0' + toString(i * 11 % 60), 2) + ':00'
})
WITH t, i, store_id
MATCH (s:Store {id: store_id})
CREATE (t)-[:AT_STORE]->(s)
WITH t, i
MATCH (cu:Customer {id: 'cu-' + right('000' + toString((i % 50) + 1), 3)})
CREATE (cu)-[:PURCHASED]->(t)
WITH t, i
MATCH (p:Product {id: 'p' + right('000' + toString(((i * 13) % 100) + 1), 3)})
CREATE (t)-[:CONTAINS {quantity: 1 + i % 3, unit_price: p.price}]->(p);

// --- Reviews (80) ---
UNWIND range(1, 80) AS i
WITH i,
  CASE i % 5 WHEN 0 THEN 5 WHEN 1 THEN 4 WHEN 2 THEN 5 WHEN 3 THEN 3 ELSE 4 END AS rating,
  CASE i % 10
    WHEN 0 THEN '정말 좋아요! 피부가 확 달라졌어요'
    WHEN 1 THEN '가성비 최고입니다. 재구매 의사 있어요'
    WHEN 2 THEN '향이 좋고 발림성이 부드러워요'
    WHEN 3 THEN '보통이에요. 기대만큼은 아니었어요'
    WHEN 4 THEN '매일 쓰고 있는데 촉촉함이 지속돼요'
    WHEN 5 THEN '선물로 줬는데 반응이 좋았어요'
    WHEN 6 THEN '용량 대비 가격이 합리적이에요'
    WHEN 7 THEN '민감성인데 자극 없이 잘 써요'
    WHEN 8 THEN '텍스처가 가벼워서 여름에도 좋아요'
    ELSE '패키지도 예쁘고 효과도 있어요'
  END AS review_text
CREATE (rv:Review {
  id: 'rv-' + toString(i),
  rating: rating,
  text: review_text,
  created_at: '2025-' + right('0' + toString((i % 12) + 1), 2) + '-' + right('0' + toString((i % 28) + 1), 2) + 'T' + right('0' + toString(8 + i % 14), 2) + ':00:00'
})
WITH rv, i
MATCH (cu:Customer {id: 'cu-' + right('000' + toString((i % 50) + 1), 3)})
CREATE (cu)-[:WROTE]->(rv)
WITH rv, i
MATCH (p:Product {id: 'p' + right('000' + toString(((i * 7) % 100) + 1), 3)})
CREATE (rv)-[:ABOUT]->(p);

// ============================================================
// Extension — Ingredients, SkinConcerns, Regulations, Referrals
// ============================================================

// --- Constraints for new node types ---
CREATE CONSTRAINT IF NOT EXISTS FOR (ig:Ingredient) REQUIRE ig.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (sc:SkinConcern) REQUIRE sc.id IS UNIQUE;
CREATE CONSTRAINT IF NOT EXISTS FOR (reg:Regulation) REQUIRE reg.id IS UNIQUE;

// --- Skin Concerns (8) ---
CREATE (:SkinConcern {id: 'sc-dryness', name: '건조'});
CREATE (:SkinConcern {id: 'sc-acne', name: '여드름'});
CREATE (:SkinConcern {id: 'sc-aging', name: '노화'});
CREATE (:SkinConcern {id: 'sc-brightening', name: '미백'});
CREATE (:SkinConcern {id: 'sc-sensitivity', name: '민감'});
CREATE (:SkinConcern {id: 'sc-pores', name: '모공'});
CREATE (:SkinConcern {id: 'sc-pigmentation', name: '색소침착'});
CREATE (:SkinConcern {id: 'sc-elasticity', name: '탄력저하'});

// --- Ingredients (25) — realistic INCI-based cosmetic ingredients ---

// Active / Treatment
CREATE (:Ingredient {id: 'ig-retinol', name: '레티놀', name_inci: 'Retinol', ingredient_type: 'active', ewg_grade: 4});
CREATE (:Ingredient {id: 'ig-niacinamide', name: '나이아신아마이드', name_inci: 'Niacinamide', ingredient_type: 'active', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-vitc', name: '비타민C', name_inci: 'Ascorbic Acid', ingredient_type: 'antioxidant', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-vite', name: '비타민E', name_inci: 'Tocopherol', ingredient_type: 'antioxidant', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-adenosine', name: '아데노신', name_inci: 'Adenosine', ingredient_type: 'active', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-glutathione', name: '글루타치온', name_inci: 'Glutathione', ingredient_type: 'antioxidant', ewg_grade: 1});

// Moisturizer / Barrier
CREATE (:Ingredient {id: 'ig-ha', name: '히알루론산', name_inci: 'Hyaluronic Acid', ingredient_type: 'moisturizer', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-ceramide', name: '세라마이드', name_inci: 'Ceramide NP', ingredient_type: 'moisturizer', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-panthenol', name: '판테놀', name_inci: 'Panthenol', ingredient_type: 'moisturizer', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-collagen', name: '콜라겐', name_inci: 'Hydrolyzed Collagen', ingredient_type: 'moisturizer', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-snail', name: '달팽이 뮤신', name_inci: 'Snail Secretion Filtrate', ingredient_type: 'moisturizer', ewg_grade: 1});

// Exfoliant
CREATE (:Ingredient {id: 'ig-aha', name: 'AHA (글리콜산)', name_inci: 'Glycolic Acid', ingredient_type: 'exfoliant', ewg_grade: 4});
CREATE (:Ingredient {id: 'ig-bha', name: 'BHA (살리실산)', name_inci: 'Salicylic Acid', ingredient_type: 'exfoliant', ewg_grade: 3});

// Plant Extract
CREATE (:Ingredient {id: 'ig-centella', name: '센텔라', name_inci: 'Centella Asiatica Extract', ingredient_type: 'active', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-greentea', name: '녹차 추출물', name_inci: 'Camellia Sinensis Leaf Extract', ingredient_type: 'antioxidant', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-houttuynia', name: '어성초', name_inci: 'Houttuynia Cordata Extract', ingredient_type: 'active', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-rice', name: '쌀 추출물', name_inci: 'Oryza Sativa Bran Extract', ingredient_type: 'active', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-aloe', name: '알로에', name_inci: 'Aloe Barbadensis Leaf Extract', ingredient_type: 'moisturizer', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-birch', name: '자작나무 수액', name_inci: 'Betula Alba Juice', ingredient_type: 'moisturizer', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-galactomyces', name: '갈락토미세스', name_inci: 'Galactomyces Ferment Filtrate', ingredient_type: 'active', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-deepsea', name: '독도 심층수', name_inci: 'Deep Sea Water', ingredient_type: 'moisturizer', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-noni', name: '노니 추출물', name_inci: 'Morinda Citrifolia Fruit Extract', ingredient_type: 'antioxidant', ewg_grade: 1});
CREATE (:Ingredient {id: 'ig-yuzu', name: '유자 추출물', name_inci: 'Citrus Junos Fruit Extract', ingredient_type: 'antioxidant', ewg_grade: 1});

// Sunscreen Filter
CREATE (:Ingredient {id: 'ig-zincoxide', name: '징크옥사이드', name_inci: 'Zinc Oxide', ingredient_type: 'sunscreen_filter', ewg_grade: 2});
CREATE (:Ingredient {id: 'ig-tinosorb', name: '티노솔브S', name_inci: 'Bis-Ethylhexyloxyphenol Methoxyphenyl Triazine', ingredient_type: 'sunscreen_filter', ewg_grade: 3});

// --- Ingredient → SkinConcern (TREATS) ---
// Retinol: anti-aging, brightening, acne
MATCH (i:Ingredient {id: 'ig-retinol'}), (s:SkinConcern {id: 'sc-aging'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-retinol'}), (s:SkinConcern {id: 'sc-brightening'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);
MATCH (i:Ingredient {id: 'ig-retinol'}), (s:SkinConcern {id: 'sc-acne'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);
MATCH (i:Ingredient {id: 'ig-retinol'}), (s:SkinConcern {id: 'sc-elasticity'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);

// Niacinamide: brightening, pores, pigmentation
MATCH (i:Ingredient {id: 'ig-niacinamide'}), (s:SkinConcern {id: 'sc-brightening'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-niacinamide'}), (s:SkinConcern {id: 'sc-pores'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);
MATCH (i:Ingredient {id: 'ig-niacinamide'}), (s:SkinConcern {id: 'sc-pigmentation'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);

// Vitamin C: brightening, antioxidant, pigmentation
MATCH (i:Ingredient {id: 'ig-vitc'}), (s:SkinConcern {id: 'sc-brightening'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-vitc'}), (s:SkinConcern {id: 'sc-pigmentation'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-vitc'}), (s:SkinConcern {id: 'sc-aging'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Vitamin E: aging
MATCH (i:Ingredient {id: 'ig-vite'}), (s:SkinConcern {id: 'sc-aging'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Adenosine: aging, elasticity
MATCH (i:Ingredient {id: 'ig-adenosine'}), (s:SkinConcern {id: 'sc-aging'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-adenosine'}), (s:SkinConcern {id: 'sc-elasticity'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);

// Glutathione: brightening, pigmentation
MATCH (i:Ingredient {id: 'ig-glutathione'}), (s:SkinConcern {id: 'sc-brightening'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-glutathione'}), (s:SkinConcern {id: 'sc-pigmentation'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Hyaluronic Acid: dryness
MATCH (i:Ingredient {id: 'ig-ha'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);

// Ceramide: dryness, sensitivity
MATCH (i:Ingredient {id: 'ig-ceramide'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-ceramide'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);

// Panthenol: sensitivity, dryness
MATCH (i:Ingredient {id: 'ig-panthenol'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-panthenol'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Collagen: elasticity, aging
MATCH (i:Ingredient {id: 'ig-collagen'}), (s:SkinConcern {id: 'sc-elasticity'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-collagen'}), (s:SkinConcern {id: 'sc-aging'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Snail Mucin: dryness, aging, sensitivity
MATCH (i:Ingredient {id: 'ig-snail'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-snail'}), (s:SkinConcern {id: 'sc-aging'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);
MATCH (i:Ingredient {id: 'ig-snail'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// AHA: pores, pigmentation
MATCH (i:Ingredient {id: 'ig-aha'}), (s:SkinConcern {id: 'sc-pores'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-aha'}), (s:SkinConcern {id: 'sc-pigmentation'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// BHA: acne, pores
MATCH (i:Ingredient {id: 'ig-bha'}), (s:SkinConcern {id: 'sc-acne'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-bha'}), (s:SkinConcern {id: 'sc-pores'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);

// Centella: sensitivity, acne
MATCH (i:Ingredient {id: 'ig-centella'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-centella'}), (s:SkinConcern {id: 'sc-acne'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Green Tea: aging (antioxidant)
MATCH (i:Ingredient {id: 'ig-greentea'}), (s:SkinConcern {id: 'sc-aging'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Houttuynia: acne, sensitivity
MATCH (i:Ingredient {id: 'ig-houttuynia'}), (s:SkinConcern {id: 'sc-acne'}) CREATE (i)-[:TREATS {efficacy_level: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-houttuynia'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Rice: brightening
MATCH (i:Ingredient {id: 'ig-rice'}), (s:SkinConcern {id: 'sc-brightening'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Aloe: sensitivity, dryness
MATCH (i:Ingredient {id: 'ig-aloe'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);
MATCH (i:Ingredient {id: 'ig-aloe'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// Galactomyces: brightening, pores
MATCH (i:Ingredient {id: 'ig-galactomyces'}), (s:SkinConcern {id: 'sc-brightening'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);
MATCH (i:Ingredient {id: 'ig-galactomyces'}), (s:SkinConcern {id: 'sc-pores'}) CREATE (i)-[:TREATS {efficacy_level: 'medium'}]->(s);

// --- Ingredient → SkinConcern (AGGRAVATES) ---
// Retinol aggravates sensitivity
MATCH (i:Ingredient {id: 'ig-retinol'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:AGGRAVATES {severity: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-retinol'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:AGGRAVATES {severity: 'medium'}]->(s);

// AHA aggravates sensitivity
MATCH (i:Ingredient {id: 'ig-aha'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:AGGRAVATES {severity: 'high'}]->(s);
MATCH (i:Ingredient {id: 'ig-aha'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:AGGRAVATES {severity: 'medium'}]->(s);

// BHA aggravates sensitivity (mild)
MATCH (i:Ingredient {id: 'ig-bha'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:AGGRAVATES {severity: 'medium'}]->(s);
MATCH (i:Ingredient {id: 'ig-bha'}), (s:SkinConcern {id: 'sc-dryness'}) CREATE (i)-[:AGGRAVATES {severity: 'low'}]->(s);

// Vitamin C (L-ascorbic acid) can irritate sensitive skin
MATCH (i:Ingredient {id: 'ig-vitc'}), (s:SkinConcern {id: 'sc-sensitivity'}) CREATE (i)-[:AGGRAVATES {severity: 'medium'}]->(s);

// --- Ingredient ↔ Ingredient (SYNERGIZES_WITH) ---
// Vitamin C + Vitamin E: 4x antioxidant synergy (Duke et al.)
MATCH (a:Ingredient {id: 'ig-vitc'}), (b:Ingredient {id: 'ig-vite'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 400, mechanism: '비타민C+E 병용 시 자외선 방어력 4배 증폭 (Duke 연구)'}]->(b);

// Niacinamide + Hyaluronic Acid: hydration + barrier boost
MATCH (a:Ingredient {id: 'ig-niacinamide'}), (b:Ingredient {id: 'ig-ha'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 50, mechanism: '수분 공급 + 피부 장벽 강화 동시 효과'}]->(b);

// Centella + Panthenol: calming synergy
MATCH (a:Ingredient {id: 'ig-centella'}), (b:Ingredient {id: 'ig-panthenol'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 80, mechanism: '진정 + 재생 시너지, 손상 피부 회복 가속'}]->(b);

// Ceramide + Hyaluronic Acid: barrier + hydration
MATCH (a:Ingredient {id: 'ig-ceramide'}), (b:Ingredient {id: 'ig-ha'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 60, mechanism: '장벽 복구(세라마이드) + 수분 유지(히알루론산) 상호 보완'}]->(b);

// Retinol + Adenosine: anti-aging boost
MATCH (a:Ingredient {id: 'ig-retinol'}), (b:Ingredient {id: 'ig-adenosine'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 70, mechanism: '세포 턴오버(레티놀) + 콜라겐 합성(아데노신) 이중 항노화'}]->(b);

// Niacinamide + Retinol: tolerability improvement
MATCH (a:Ingredient {id: 'ig-niacinamide'}), (b:Ingredient {id: 'ig-retinol'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 30, mechanism: '나이아신아마이드가 레티놀의 자극을 완화하면서 효과 유지'}]->(b);

// Vitamin C + Centella: brightening + repair
MATCH (a:Ingredient {id: 'ig-vitc'}), (b:Ingredient {id: 'ig-centella'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 40, mechanism: '미백(비타민C) + 진정(센텔라)으로 톤업 시 자극 최소화'}]->(b);

// Snail Mucin + Hyaluronic Acid: deep hydration
MATCH (a:Ingredient {id: 'ig-snail'}), (b:Ingredient {id: 'ig-ha'}) CREATE (a)-[:SYNERGIZES_WITH {boost_pct: 50, mechanism: '뮤신 보습막 + 히알루론산 수분 흡착의 이중 보습'}]->(b);

// --- Ingredient ↔ Ingredient (CONFLICTS_WITH) ---
// Retinol + AHA: over-exfoliation risk
MATCH (a:Ingredient {id: 'ig-retinol'}), (b:Ingredient {id: 'ig-aha'}) CREATE (a)-[:CONFLICTS_WITH {risk_level: 'high', reason: '레티놀과 AHA 동시 사용 시 과도한 각질 제거로 피부 장벽 손상 위험'}]->(b);

// Retinol + BHA: over-exfoliation risk
MATCH (a:Ingredient {id: 'ig-retinol'}), (b:Ingredient {id: 'ig-bha'}) CREATE (a)-[:CONFLICTS_WITH {risk_level: 'high', reason: '레티놀과 BHA 동시 사용 시 과도한 자극, 특히 민감성 피부에 위험'}]->(b);

// AHA + Vitamin C: pH conflict
MATCH (a:Ingredient {id: 'ig-aha'}), (b:Ingredient {id: 'ig-vitc'}) CREATE (a)-[:CONFLICTS_WITH {risk_level: 'medium', reason: '둘 다 낮은 pH 필요, 동시 사용 시 자극 증가. 시간차 사용 권장 (아침/저녁 분리)'}]->(b);

// Retinol + Vitamin C: traditionally avoided (pH conflict)
MATCH (a:Ingredient {id: 'ig-retinol'}), (b:Ingredient {id: 'ig-vitc'}) CREATE (a)-[:CONFLICTS_WITH {risk_level: 'medium', reason: 'pH 환경 차이로 효능 저하 가능. 아침(비타민C)/저녁(레티놀) 분리 사용 권장'}]->(b);

// AHA + BHA: combined over-exfoliation
MATCH (a:Ingredient {id: 'ig-aha'}), (b:Ingredient {id: 'ig-bha'}) CREATE (a)-[:CONFLICTS_WITH {risk_level: 'medium', reason: '이중 산성 각질제거제 동시 사용 시 과도한 자극. 번갈아 사용 권장'}]->(b);

// --- Regulations (6) ---
CREATE (:Regulation {id: 'reg-eu-retinol', name: 'EU 레티놀 농도 제한', authority: 'EU_SCCS', status: 'restricted', effective_date: '2025-05-01'});
CREATE (:Regulation {id: 'reg-eu-aha', name: 'EU AHA 농도 제한 (일반화장품)', authority: 'EU_SCCS', status: 'restricted', effective_date: '2020-01-01'});
CREATE (:Regulation {id: 'reg-kr-retinol', name: '식약처 레티놀 배합 한도', authority: 'MFDS', status: 'restricted', effective_date: '2023-07-01'});
CREATE (:Regulation {id: 'reg-kr-aha', name: '식약처 AHA 농도 기준', authority: 'MFDS', status: 'restricted', effective_date: '2020-06-01'});
CREATE (:Regulation {id: 'reg-eu-zincoxide', name: 'EU 나노 징크옥사이드 규제', authority: 'EU_SCCS', status: 'restricted', effective_date: '2024-01-01'});
CREATE (:Regulation {id: 'reg-fda-sunscreen', name: 'FDA 선스크린 필터 규제', authority: 'FDA', status: 'restricted', effective_date: '2024-09-01'});

// --- Ingredient → Regulation (REGULATED_BY) ---
MATCH (i:Ingredient {id: 'ig-retinol'}), (r:Regulation {id: 'reg-eu-retinol'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 0.3}]->(r);
MATCH (i:Ingredient {id: 'ig-retinol'}), (r:Regulation {id: 'reg-kr-retinol'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 0.5}]->(r);
MATCH (i:Ingredient {id: 'ig-aha'}), (r:Regulation {id: 'reg-eu-aha'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 10.0}]->(r);
MATCH (i:Ingredient {id: 'ig-aha'}), (r:Regulation {id: 'reg-kr-aha'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 10.0}]->(r);
MATCH (i:Ingredient {id: 'ig-zincoxide'}), (r:Regulation {id: 'reg-eu-zincoxide'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 25.0}]->(r);
MATCH (i:Ingredient {id: 'ig-tinosorb'}), (r:Regulation {id: 'reg-fda-sunscreen'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 10.0}]->(r);
MATCH (i:Ingredient {id: 'ig-zincoxide'}), (r:Regulation {id: 'reg-fda-sunscreen'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 25.0}]->(r);
MATCH (i:Ingredient {id: 'ig-bha'}), (r:Regulation {id: 'reg-kr-aha'}) CREATE (i)-[:REGULATED_BY {max_concentration_pct: 2.0}]->(r);

// --- Product → Ingredient (HAS_INGREDIENT) ---
// Mapping based on real product formulations

// Toners
// p001 라운드랩 독도 토너
MATCH (p:Product {id: 'p001'}), (i:Ingredient) WHERE i.id IN ['ig-deepsea','ig-ha','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-deepsea'}]->(i);
// p002 아누아 어성초 77 토너
MATCH (p:Product {id: 'p002'}), (i:Ingredient) WHERE i.id IN ['ig-houttuynia','ig-ha','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-houttuynia', concentration_pct: CASE WHEN i.id = 'ig-houttuynia' THEN 77.0 ELSE null END}]->(i);
// p003 이즈앤트리 히알루론산 토너
MATCH (p:Product {id: 'p003'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-panthenol','ig-aloe'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);
// p004 코스알엑스 AHA/BHA 토너
MATCH (p:Product {id: 'p004'}), (i:Ingredient) WHERE i.id IN ['ig-aha','ig-bha','ig-aloe'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p005 토리든 다이브인 히알루론산 토너
MATCH (p:Product {id: 'p005'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-panthenol','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);
// p006 넘버즈인 1번 토너패드
MATCH (p:Product {id: 'p006'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-niacinamide','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p007 라운드랩 소나무 진정 토너
MATCH (p:Product {id: 'p007'}), (i:Ingredient) WHERE i.id IN ['ig-deepsea','ig-centella','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-centella'}]->(i);
// p008 아비브 어성초 pH 밸런스 토너
MATCH (p:Product {id: 'p008'}), (i:Ingredient) WHERE i.id IN ['ig-houttuynia','ig-ha','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-houttuynia'}]->(i);
// p009 믹순 빈 에센스
MATCH (p:Product {id: 'p009'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-niacinamide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p010 이니스프리 그린티 씨드 스킨
MATCH (p:Product {id: 'p010'}), (i:Ingredient) WHERE i.id IN ['ig-greentea','ig-ha','ig-niacinamide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-greentea'}]->(i);
// p011 벤톤 알로에 BHA 스킨 토너
MATCH (p:Product {id: 'p011'}), (i:Ingredient) WHERE i.id IN ['ig-bha','ig-aloe','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p012 셀리맥스 노니 앰플 토너
MATCH (p:Product {id: 'p012'}), (i:Ingredient) WHERE i.id IN ['ig-noni','ig-ha','ig-niacinamide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-noni'}]->(i);

// Serums
// p013 코스알엑스 스네일 96 에센스
MATCH (p:Product {id: 'p013'}), (i:Ingredient) WHERE i.id IN ['ig-snail','ig-ha','ig-aloe'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-snail', concentration_pct: CASE WHEN i.id = 'ig-snail' THEN 96.0 ELSE null END}]->(i);
// p014 토리든 다이브인 히알루론산 세럼
MATCH (p:Product {id: 'p014'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-panthenol','ig-ceramide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);
// p015 넘버즈인 5번 비타톤 글루타치온C 세럼
MATCH (p:Product {id: 'p015'}), (i:Ingredient) WHERE i.id IN ['ig-glutathione','ig-vitc','ig-niacinamide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p016 조선미녀 맑은 쌀 뷰티 세럼
MATCH (p:Product {id: 'p016'}), (i:Ingredient) WHERE i.id IN ['ig-rice','ig-niacinamide','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-rice'}]->(i);
// p017 아누아 어성초 77 수딩 세럼
MATCH (p:Product {id: 'p017'}), (i:Ingredient) WHERE i.id IN ['ig-houttuynia','ig-panthenol','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-houttuynia', concentration_pct: CASE WHEN i.id = 'ig-houttuynia' THEN 77.0 ELSE null END}]->(i);
// p018 이즈앤트리 C 비타민 세럼
MATCH (p:Product {id: 'p018'}), (i:Ingredient) WHERE i.id IN ['ig-vitc','ig-vite','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-vitc'}]->(i);
// p019 스킨1004 센텔라 앰플
MATCH (p:Product {id: 'p019'}), (i:Ingredient) WHERE i.id IN ['ig-centella','ig-ha','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-centella'}]->(i);
// p020 믹순 갈락토미세스 발효 에센스
MATCH (p:Product {id: 'p020'}), (i:Ingredient) WHERE i.id IN ['ig-galactomyces','ig-niacinamide','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-galactomyces'}]->(i);
// p021 메디큐브 콜라겐 래핑 마스크
MATCH (p:Product {id: 'p021'}), (i:Ingredient) WHERE i.id IN ['ig-collagen','ig-adenosine','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-collagen'}]->(i);
// p022 라운드랩 자작나무 수분 세럼
MATCH (p:Product {id: 'p022'}), (i:Ingredient) WHERE i.id IN ['ig-birch','ig-ha','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-birch'}]->(i);
// p023 코스알엑스 비타민C 23 세럼
MATCH (p:Product {id: 'p023'}), (i:Ingredient) WHERE i.id IN ['ig-vitc','ig-vite','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-vitc', concentration_pct: CASE WHEN i.id = 'ig-vitc' THEN 23.0 ELSE null END}]->(i);
// p024 닥터지 레드 블레미쉬 수딩 크림
MATCH (p:Product {id: 'p024'}), (i:Ingredient) WHERE i.id IN ['ig-centella','ig-panthenol','ig-ceramide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-centella'}]->(i);
// p025 썸바이미 유자 나이아신 세럼
MATCH (p:Product {id: 'p025'}), (i:Ingredient) WHERE i.id IN ['ig-yuzu','ig-niacinamide','ig-vitc'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p026 이니스프리 레티놀 시카 세럼
MATCH (p:Product {id: 'p026'}), (i:Ingredient) WHERE i.id IN ['ig-retinol','ig-centella','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-retinol', concentration_pct: CASE WHEN i.id = 'ig-retinol' THEN 0.1 ELSE null END}]->(i);
// p027 라네즈 워터 슬리핑 마스크 EX
MATCH (p:Product {id: 'p027'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-vitc','ig-ceramide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);

// Creams
// p028 라운드랩 독도 크림
MATCH (p:Product {id: 'p028'}), (i:Ingredient) WHERE i.id IN ['ig-deepsea','ig-ceramide','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-deepsea'}]->(i);
// p029 코스알엑스 스네일 92 크림
MATCH (p:Product {id: 'p029'}), (i:Ingredient) WHERE i.id IN ['ig-snail','ig-adenosine','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-snail', concentration_pct: CASE WHEN i.id = 'ig-snail' THEN 92.0 ELSE null END}]->(i);
// p030 일리윤 세라마이드 아토 크림
MATCH (p:Product {id: 'p030'}), (i:Ingredient) WHERE i.id IN ['ig-ceramide','ig-panthenol','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ceramide'}]->(i);
// p031 토리든 다이브인 수분 크림
MATCH (p:Product {id: 'p031'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-ceramide','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);
// p032 이즈앤트리 알로에 수딩 젤
MATCH (p:Product {id: 'p032'}), (i:Ingredient) WHERE i.id IN ['ig-aloe','ig-ha','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-aloe'}]->(i);
// p033 조선미녀 조선왕조 크림
MATCH (p:Product {id: 'p033'}), (i:Ingredient) WHERE i.id IN ['ig-rice','ig-adenosine','ig-niacinamide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-rice'}]->(i);
// p034 설화수 자음생 크림
MATCH (p:Product {id: 'p034'}), (i:Ingredient) WHERE i.id IN ['ig-adenosine','ig-retinol','ig-niacinamide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-adenosine'}]->(i);
// p035 이니스프리 그린티 씨드 크림
MATCH (p:Product {id: 'p035'}), (i:Ingredient) WHERE i.id IN ['ig-greentea','ig-ha','ig-ceramide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-greentea'}]->(i);
// p036 넘버즈인 3번 콜라겐 탄력 크림
MATCH (p:Product {id: 'p036'}), (i:Ingredient) WHERE i.id IN ['ig-collagen','ig-niacinamide','ig-adenosine'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-collagen'}]->(i);
// p037 아비브 크림 코팅 마스크
MATCH (p:Product {id: 'p037'}), (i:Ingredient) WHERE i.id IN ['ig-centella','ig-panthenol','ig-ceramide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-centella'}]->(i);

// Suncare
// p038 조선미녀 맑은 쌀 선크림
MATCH (p:Product {id: 'p038'}), (i:Ingredient) WHERE i.id IN ['ig-rice','ig-zincoxide','ig-niacinamide'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p039 라운드랩 자작나무 수분 선크림
MATCH (p:Product {id: 'p039'}), (i:Ingredient) WHERE i.id IN ['ig-birch','ig-tinosorb','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p040 토리든 다이브인 워터리 선크림
MATCH (p:Product {id: 'p040'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-tinosorb','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p041 이즈앤트리 히알루론산 워터리 선젤
MATCH (p:Product {id: 'p041'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-tinosorb','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);
// p042 스킨1004 센텔라 선크림
MATCH (p:Product {id: 'p042'}), (i:Ingredient) WHERE i.id IN ['ig-centella','ig-zincoxide','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p043 닥터지 그린 마일드 업 선
MATCH (p:Product {id: 'p043'}), (i:Ingredient) WHERE i.id IN ['ig-centella','ig-zincoxide','ig-aloe'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p044 셀리맥스 노니 선크림
MATCH (p:Product {id: 'p044'}), (i:Ingredient) WHERE i.id IN ['ig-noni','ig-tinosorb','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-noni'}]->(i);
// p045 아비브 선스틱
MATCH (p:Product {id: 'p045'}), (i:Ingredient) WHERE i.id IN ['ig-centella','ig-zincoxide','ig-vite'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);

// Masks
// p046 메디힐 N.M.F 마스크
MATCH (p:Product {id: 'p046'}), (i:Ingredient) WHERE i.id IN ['ig-ha','ig-ceramide','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ha'}]->(i);
// p047 아비브 약산성 pH 시트마스크
MATCH (p:Product {id: 'p047'}), (i:Ingredient) WHERE i.id IN ['ig-centella','ig-panthenol','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-centella'}]->(i);
// p049 코스알엑스 아크네 패치
MATCH (p:Product {id: 'p049'}), (i:Ingredient) WHERE i.id IN ['ig-bha','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p050 라운드랩 독도 머드팩
MATCH (p:Product {id: 'p050'}), (i:Ingredient) WHERE i.id IN ['ig-deepsea','ig-aha','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-deepsea'}]->(i);

// Cleansers
// p051 라운드랩 독도 클렌징 오일
MATCH (p:Product {id: 'p051'}), (i:Ingredient) WHERE i.id IN ['ig-deepsea','ig-vite'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-deepsea'}]->(i);
// p052 코스알엑스 로우pH 굿모닝 클렌저
MATCH (p:Product {id: 'p052'}), (i:Ingredient) WHERE i.id IN ['ig-bha','ig-greentea','ig-centella'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);
// p054 아누아 어성초 클렌징 오일
MATCH (p:Product {id: 'p054'}), (i:Ingredient) WHERE i.id IN ['ig-houttuynia','ig-vite'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-houttuynia'}]->(i);

// Creams/Body (select key products)
// p030 일리윤 세라마이드 (already done above)
// p089 일리윤 세라마이드 바디워시
MATCH (p:Product {id: 'p089'}), (i:Ingredient) WHERE i.id IN ['ig-ceramide','ig-panthenol'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ceramide'}]->(i);
// p091 일리윤 세라마이드 바디로션
MATCH (p:Product {id: 'p091'}), (i:Ingredient) WHERE i.id IN ['ig-ceramide','ig-panthenol','ig-ha'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: i.id = 'ig-ceramide'}]->(i);
// p094 코스알엑스 AHA/BHA 바디스프레이
MATCH (p:Product {id: 'p094'}), (i:Ingredient) WHERE i.id IN ['ig-aha','ig-bha','ig-aloe'] CREATE (p)-[:HAS_INGREDIENT {is_key_ingredient: true}]->(i);

// --- Customer → Customer (REFERRED) — Referral network ---
// Root influencers (no incoming REFERRED): cu-001 (김지현), cu-006 (강예림), cu-017 (장수아), cu-020 (유하린)

// Chain 1: cu-001 → cu-004, cu-008, cu-016; cu-004 → cu-029, cu-035; cu-008 → cu-047
MATCH (a:Customer {id: 'cu-001'}), (b:Customer {id: 'cu-004'}) CREATE (a)-[:REFERRED {referred_at: '2023-01-05'}]->(b);
MATCH (a:Customer {id: 'cu-001'}), (b:Customer {id: 'cu-008'}) CREATE (a)-[:REFERRED {referred_at: '2023-05-10'}]->(b);
MATCH (a:Customer {id: 'cu-001'}), (b:Customer {id: 'cu-016'}) CREATE (a)-[:REFERRED {referred_at: '2023-07-01'}]->(b);
MATCH (a:Customer {id: 'cu-004'}), (b:Customer {id: 'cu-029'}) CREATE (a)-[:REFERRED {referred_at: '2023-04-20'}]->(b);
MATCH (a:Customer {id: 'cu-004'}), (b:Customer {id: 'cu-035'}) CREATE (a)-[:REFERRED {referred_at: '2023-06-25'}]->(b);
MATCH (a:Customer {id: 'cu-008'}), (b:Customer {id: 'cu-047'}) CREATE (a)-[:REFERRED {referred_at: '2023-08-10'}]->(b);

// Chain 2: cu-006 → cu-010, cu-015, cu-021; cu-010 → cu-019, cu-026; cu-015 → cu-032, cu-039; cu-019 → cu-042
MATCH (a:Customer {id: 'cu-006'}), (b:Customer {id: 'cu-010'}) CREATE (a)-[:REFERRED {referred_at: '2021-02-10'}]->(b);
MATCH (a:Customer {id: 'cu-006'}), (b:Customer {id: 'cu-015'}) CREATE (a)-[:REFERRED {referred_at: '2021-05-12'}]->(b);
MATCH (a:Customer {id: 'cu-006'}), (b:Customer {id: 'cu-021'}) CREATE (a)-[:REFERRED {referred_at: '2021-03-18'}]->(b);
MATCH (a:Customer {id: 'cu-010'}), (b:Customer {id: 'cu-019'}) CREATE (a)-[:REFERRED {referred_at: '2022-11-28'}]->(b);
MATCH (a:Customer {id: 'cu-010'}), (b:Customer {id: 'cu-026'}) CREATE (a)-[:REFERRED {referred_at: '2023-03-14'}]->(b);
MATCH (a:Customer {id: 'cu-015'}), (b:Customer {id: 'cu-032'}) CREATE (a)-[:REFERRED {referred_at: '2021-11-15'}]->(b);
MATCH (a:Customer {id: 'cu-015'}), (b:Customer {id: 'cu-039'}) CREATE (a)-[:REFERRED {referred_at: '2021-12-20'}]->(b);
MATCH (a:Customer {id: 'cu-019'}), (b:Customer {id: 'cu-042'}) CREATE (a)-[:REFERRED {referred_at: '2023-01-28'}]->(b);

// Chain 3: cu-017 → cu-024, cu-030, cu-038; cu-024 → cu-033, cu-044; cu-030 → cu-036, cu-046
MATCH (a:Customer {id: 'cu-017'}), (b:Customer {id: 'cu-024'}) CREATE (a)-[:REFERRED {referred_at: '2019-01-15'}]->(b);
MATCH (a:Customer {id: 'cu-017'}), (b:Customer {id: 'cu-030'}) CREATE (a)-[:REFERRED {referred_at: '2019-06-10'}]->(b);
MATCH (a:Customer {id: 'cu-017'}), (b:Customer {id: 'cu-038'}) CREATE (a)-[:REFERRED {referred_at: '2019-10-12'}]->(b);
MATCH (a:Customer {id: 'cu-024'}), (b:Customer {id: 'cu-033'}) CREATE (a)-[:REFERRED {referred_at: '2020-08-01'}]->(b);
MATCH (a:Customer {id: 'cu-024'}), (b:Customer {id: 'cu-044'}) CREATE (a)-[:REFERRED {referred_at: '2021-10-05'}]->(b);
MATCH (a:Customer {id: 'cu-030'}), (b:Customer {id: 'cu-036'}) CREATE (a)-[:REFERRED {referred_at: '2021-08-08'}]->(b);
MATCH (a:Customer {id: 'cu-030'}), (b:Customer {id: 'cu-046'}) CREATE (a)-[:REFERRED {referred_at: '2021-06-25'}]->(b);

// Chain 4: cu-020 → cu-007, cu-013, cu-031, cu-040, cu-049; cu-007 → cu-022, cu-037; cu-013 → cu-028
MATCH (a:Customer {id: 'cu-020'}), (b:Customer {id: 'cu-007'}) CREATE (a)-[:REFERRED {referred_at: '2020-07-25'}]->(b);
MATCH (a:Customer {id: 'cu-020'}), (b:Customer {id: 'cu-013'}) CREATE (a)-[:REFERRED {referred_at: '2022-03-30'}]->(b);
MATCH (a:Customer {id: 'cu-020'}), (b:Customer {id: 'cu-031'}) CREATE (a)-[:REFERRED {referred_at: '2022-01-03'}]->(b);
MATCH (a:Customer {id: 'cu-020'}), (b:Customer {id: 'cu-040'}) CREATE (a)-[:REFERRED {referred_at: '2020-05-05'}]->(b);
MATCH (a:Customer {id: 'cu-020'}), (b:Customer {id: 'cu-049'}) CREATE (a)-[:REFERRED {referred_at: '2022-10-08'}]->(b);
MATCH (a:Customer {id: 'cu-007'}), (b:Customer {id: 'cu-022'}) CREATE (a)-[:REFERRED {referred_at: '2022-06-12'}]->(b);
MATCH (a:Customer {id: 'cu-007'}), (b:Customer {id: 'cu-037'}) CREATE (a)-[:REFERRED {referred_at: '2022-11-01'}]->(b);
MATCH (a:Customer {id: 'cu-013'}), (b:Customer {id: 'cu-028'}) CREATE (a)-[:REFERRED {referred_at: '2022-08-28'}]->(b);
