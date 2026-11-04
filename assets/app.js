// The runtime tabs. No framework, and it works with the keyboard.

const tabs = Array.from(document.querySelectorAll('[role="tab"]'));
const panels = Array.from(document.querySelectorAll('[role="tabpanel"]'));

function select(tab) {
  for (const other of tabs) {
    const active = other === tab;
    other.setAttribute('aria-selected', String(active));
    other.tabIndex = active ? 0 : -1;
  }
  for (const panel of panels) {
    panel.hidden = panel.getAttribute('aria-labelledby') !== tab.id;
  }
}

for (const tab of tabs) {
  tab.addEventListener('click', () => select(tab));
  tab.addEventListener('keydown', (event) => {
    const index = tabs.indexOf(tab);
    const next =
      event.key === 'ArrowRight' ? (index + 1) % tabs.length :
      event.key === 'ArrowLeft' ? (index - 1 + tabs.length) % tabs.length :
      null;
    if (next === null) return;
    event.preventDefault();
    tabs[next].focus();
    select(tabs[next]);
  });
}

select(tabs[0]);
