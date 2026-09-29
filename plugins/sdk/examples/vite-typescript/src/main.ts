import "./style.css";

type PackageSummary = {
  id: string;
  displayName: string;
  description: string;
  enabled: boolean;
};

/** Loads the plugin catalog exposed by the Vite server. */
async function loadCatalog(): Promise<PackageSummary[]> {
  const response = await fetch("/api/packages");
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return (await response.json()) as PackageSummary[];
}

/** Renders one catalog response into the page. */
function renderCatalog(packages: PackageSummary[]): void {
  const catalog = document.querySelector<HTMLDivElement>("#catalog");
  if (!catalog) {
    throw new Error("Catalog element is missing");
  }
  catalog.replaceChildren(
    ...packages.map((item) => {
      const card = document.createElement("article");
      card.className = "package-card";
      const title = document.createElement("h2");
      title.textContent = `${item.displayName} · ${item.id}`;
      const description = document.createElement("p");
      description.textContent = `${item.description} · ${item.enabled ? "enabled" : "disabled"}`;
      card.append(title, description);
      return card;
    }),
  );
}

/** Requests the catalog and updates loading and error state. */
async function refreshCatalog(): Promise<void> {
  const status = document.querySelector<HTMLParagraphElement>("#status");
  const refresh = document.querySelector<HTMLButtonElement>("#refresh");
  if (!status || !refresh) {
    throw new Error("Catalog controls are missing");
  }
  refresh.disabled = true;
  status.textContent = "Loading plugin catalog…";
  try {
    const packages = await loadCatalog();
    renderCatalog(packages);
    status.textContent = `${packages.length} package${packages.length === 1 ? "" : "s"} returned by the SDK.`;
  } catch (error) {
    status.textContent = error instanceof Error ? error.message : String(error);
  } finally {
    refresh.disabled = false;
  }
}

document.querySelector<HTMLButtonElement>("#refresh")?.addEventListener("click", () => {
  void refreshCatalog();
});
void refreshCatalog();
