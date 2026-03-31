# Google SSO Setup Guide

## 1. Create Google OAuth Client

1. Go to [Google Cloud Console Credentials](https://console.cloud.google.com/apis/credentials)
2. Select your project (or create one)
3. Click **"+ CREATE CREDENTIALS"** → **"OAuth client ID"**
4. If prompted, configure the **OAuth consent screen**:
   - User Type: **External** (or Internal for Google Workspace)
   - App name: **Ontosyx**
   - User support email: your email
   - Authorized domains: `localhost` (for development)
   - Developer contact: your email
5. Create OAuth Client:
   - Application type: **Web application**
   - Name: **Ontosyx Local Dev**
   - Authorized JavaScript origins: `http://localhost:3000`
   - Authorized redirect URIs: `http://localhost:3000/auth/callback`
6. Copy the **Client ID** and **Client Secret**

## 2. Configure Environment

Create or update `web/.env.local`:

```env
# Google OIDC
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret

# JWT secret (generate with: openssl rand -base64 32)
AUTH_JWT_SECRET=your-random-secret-at-least-32-chars

# App URL
NEXTAUTH_URL=http://localhost:3000

# Backend (already configured)
OX_API_KEY=test-key-for-verification
```

## 3. Configure Backend

Update `ontosyx.toml`:

```toml
[auth]
jwt_secret = "same-secret-as-AUTH_JWT_SECRET"
google_client_id = "same-as-GOOGLE_CLIENT_ID"
session_hours = 24
```

Or via environment variables:
```bash
export OX_AUTH__JWT_SECRET="your-secret"
export OX_AUTH__GOOGLE_CLIENT_ID="your-client-id"
```

## 4. Restart Services

```bash
# Backend
cargo build --release
OX_API_KEY=test-key-for-verification ./target/release/ontosyx

# Frontend
cd web && npx next dev
```

## 5. Test

1. Open `http://localhost:3000`
2. You should be redirected to `/login`
3. Click **"Sign in with Google"**
4. Authorize with your Google account
5. You should be redirected back to the workbench

## Dev Mode (No Auth)

If `GOOGLE_CLIENT_ID` is not set, auth is completely disabled.
The app works exactly as before — no login required, localStorage UUID for user scoping.
