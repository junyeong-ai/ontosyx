import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  listRecipes: vi.fn(),
  createRecipe: vi.fn(),
  deleteRecipe: vi.fn(),
  listRecipeVersions: vi.fn(),
  updateRecipeStatus: vi.fn(),
}));

const confirmMock = vi.fn();
vi.mock("@/components/providers/confirm-provider", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

// The RecipeRunner renders a modal with heavy editor dependencies —
// stub it out to keep the test focused on the list/details surface.
vi.mock("@/components/recipes/recipe-runner", () => ({
  RecipeRunner: () => <div data-testid="recipe-runner" />,
}));

import { RecipesWorkbench } from "@/components/recipes/recipes-workbench";
import * as api from "@/lib/api";
import { useAuth } from "@/hooks/use-auth";
import type { AnalysisRecipe } from "@/types/api";
import { toast } from "sonner";

function sampleRecipe(overrides: Partial<AnalysisRecipe> = {}): AnalysisRecipe {
  return {
    id: "rcp-1",
    name: "Customer cohort over time",
    description: "Segments customers by first purchase quarter",
    algorithm_type: "time_series",
    code_template: "print('hello')",
    parameters: {},
    required_columns: ["customer_id", "purchase_date"],
    output_description: "cohort table",
    created_by: "analyst-a",
    created_at: "2026-04-22T00:00:00Z",
    version: 1,
    status: "approved",
    parent_id: null,
    ...overrides,
  };
}

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <RecipesWorkbench />
    </NextIntlClientProvider>
  );
  render(ui);
}

function asAdmin(): void {
  vi.mocked(useAuth).mockReturnValue({
    user: { sub: "u1", email: "a@b.c", name: "Admin", role: "admin", auth_enabled: true },
    loading: false,
    isAuthenticated: true,
    authEnabled: true,
    isAdmin: true,
    canWrite: true,
  } as ReturnType<typeof useAuth>);
}

describe("RecipesWorkbench", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(api.listRecipes).mockReset();
    vi.mocked(api.deleteRecipe).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
    asAdmin();
  });

  it("renders the recipe card title from the API list", async () => {
    vi.mocked(api.listRecipes).mockResolvedValueOnce({
      items: [sampleRecipe()],
      next_cursor: undefined,
    });
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText("Customer cohort over time"),
      ).toBeInTheDocument(),
    );
  });

  it("renders the `emptyAll` copy when zero recipes are present", async () => {
    vi.mocked(api.listRecipes).mockResolvedValueOnce({
      items: [],
      next_cursor: undefined,
    });
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(/No recipes yet/i),
      ).toBeInTheDocument(),
    );
  });

  it("searching for a non-matching term shows the filtered empty copy", async () => {
    vi.mocked(api.listRecipes).mockResolvedValueOnce({
      items: [sampleRecipe()],
      next_cursor: undefined,
    });
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText("Customer cohort over time"),
      ).toBeInTheDocument(),
    );
    // The search input is the only input at the top — target by placeholder.
    fireEvent.change(screen.getByPlaceholderText(/Search/i), {
      target: { value: "nothing-matches-this" },
    });
    // When `recipes.length > 0` but `filtered.length === 0`, the
    // page swaps to the `emptyFiltered` copy.
    await waitFor(() =>
      expect(
        screen.getByText(/No matching recipes/i),
      ).toBeInTheDocument(),
    );
  });

  it("confirm=true on delete fires deleteRecipe with the id", async () => {
    vi.mocked(api.listRecipes).mockResolvedValueOnce({
      items: [sampleRecipe()],
      next_cursor: undefined,
    });
    vi.mocked(api.deleteRecipe).mockResolvedValueOnce(undefined);
    confirmMock.mockResolvedValueOnce(true);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText("Customer cohort over time"),
      ).toBeInTheDocument(),
    );
    // Click the card to open the detail panel where the Delete button lives.
    fireEvent.click(screen.getByText("Customer cohort over time"));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /^Delete$/ }),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() =>
      expect(api.deleteRecipe).toHaveBeenCalledWith("rcp-1"),
    );
    expect(toast.success).toHaveBeenCalled();
  });
});
