const entities: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#039;",
};

export const escapehtml = (value: string) =>
  value.replace(/[&<>"']/g, (character) => entities[character] ?? character);

export const code = (language: string, value: string) => `
  <div class="codeblock">
    <div class="codebar">
      <span>${escapehtml(language)}</span>
      <button type="button" data-copy aria-live="polite">Copy</button>
    </div>
    <pre><code>${escapehtml(value.trim())}</code></pre>
  </div>`;

export const note = (title: string, body: string) => `
  <aside class="note">
    <strong>${escapehtml(title)}</strong>
    <p>${body}</p>
  </aside>`;

export const command = (name: string, usage: string, body: string) => `
  <section class="command" id="${name.replaceAll(" ", "")}">
    <div>
      <h3>${escapehtml(name)}</h3>
      <code>${escapehtml(usage)}</code>
    </div>
    <p>${body}</p>
  </section>`;
