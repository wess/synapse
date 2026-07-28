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
  const input = one("#search");
  const results = one("[data-results]");
  const status = one("[data-searchstatus]");
  if (!input || !results || !status) return;
  const root = document.body.dataset.root ?? "";
  const entries = await fetch(`${root}search.json`).then((response) => response.json());
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
    }
  });
};

const scenes = () => {
  const index = one("[data-scene]");
  const takes = all("[data-take]");
  if (!index || !takes.length || !("IntersectionObserver" in window)) return;
  const observer = new IntersectionObserver(
    (entries) => {
      const active = entries.find((entry) => entry.isIntersecting);
      if (active) index.textContent = active.target.dataset.take;
    },
    { rootMargin: "-35% 0px -55%", threshold: 0 },
  );
  takes.forEach((take) => observer.observe(take));
};

togglenav();
copycode();
compactguides();
searchdocs().catch(() => undefined);
scenes();
