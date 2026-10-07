import { defineConfig } from "vite";

// The backend is mocked by the browser test harness (#76); #78 adds the
// same-origin /api proxy to the real backend.
export default defineConfig({});
