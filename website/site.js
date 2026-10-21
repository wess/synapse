const one = (selector, root = document) => root.querySelector(selector);
const all = (selector, root = document) => [...root.querySelectorAll(selector)];

const togglenav = () => {
  const button = one(".navtoggle");
  const navigation = one("#topnav");
  if (!button || !navigation) return;
  button.addEventListener("click", () => {
    const open = button.getAttribute("aria-expanded") !== "true";
    button.setAttribute("aria-expanded", String(open));
    navigation.toggleAttribute("data-open", open);
  });
};

const copycode = () => {
  for (const button of all("[data-copy]")) {
    button.addEventListener("click", async () => {
      const code = one("code", button.closest(".codeblock"));
      if (!code) return;
      try {
        await navigator.clipboard.writeText(code.textContent ?? "");
        button.textContent = "Copied";
      } catch {
        button.textContent = "Copy failed";
      }
      window.setTimeout(() => (button.textContent = "Copy"), 1600);
    });
  }
};

const compactguides = () => {
  const guides = one(".railnav");
  if (!guides || !("matchMedia" in window)) return;
  const media = window.matchMedia("(max-width: 860px)");
  const sync = () => guides.toggleAttribute("open", !media.matches);
  sync();
  media.addEventListener("change", sync);
};

const searchdocs = async () => {
  const input = one("#docssearch");
  const results = one("[data-results]");
  const status = one("[data-searchstatus]");
  if (!input || !results || !status) return;
  const root = document.body.dataset.root ?? "";
  input.disabled = true;
  input.placeholder = "Loading search…";
  let entries;
  try {
    const response = await fetch(`${root}search.json`);
    if (!response.ok) throw new Error("search unavailable");
    entries = await response.json();
  } catch {
    input.disabled = false;
    input.placeholder = "Search unavailable";
    status.textContent = "Search could not load. Browse the guides below or reload to try again.";
    return;
  }
  input.disabled = false;
  input.placeholder = "Memory, vaults, restore…";
  input.addEventListener("input", () => {
    const query = input.value.trim().toLowerCase();
    if (query.length < 2) {
      results.hidden = true;
      results.replaceChildren();
      status.textContent = "";
      return;
    }
    const matches = entries
      .map((entry) => ({
        ...entry,
        score:
          (entry.title.toLowerCase().includes(query) ? 4 : 0) +
          (entry.description.toLowerCase().includes(query) ? 2 : 0) +
          (entry.text.toLowerCase().includes(query) ? 1 : 0),
      }))
      .filter((entry) => entry.score > 0)
      .sort((left, right) => right.score - left.score)
      .slice(0, 6);
    results.innerHTML = matches.length
      ? matches
          .map(
            (entry) =>
              `<a href="${root}${entry.path}"><strong>${entry.title}</strong><span>${entry.description}</span></a>`,
          )
          .join("")
      : '<span class="searchempty">No matching guide. Try another phrase.</span>';
    results.hidden = false;
    status.textContent = matches.length
      ? `${matches.length} matching ${matches.length === 1 ? "guide" : "guides"}.`
      : "No matching guide.";
  });
  input.addEventListener("focus", () => {
    if (input.value.trim().length >= 2) input.dispatchEvent(new Event("input"));
  });
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".searchbox")) {
      results.hidden = true;
      status.textContent = "";
    }
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      results.hidden = true;
      status.textContent = "";
    } else if (event.key === "ArrowDown" && !results.hidden) {
      event.preventDefault();
      one("a", results)?.focus();
    }
  });
  results.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      input.focus();
      results.hidden = true;
    }
  });
};

togglenav();
copycode();
compactguides();
searchdocs();
