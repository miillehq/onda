/** Measure the space a preview leaves unpainted in its card, excluding the intended padding. */
export function unusedPreviewWidths(): number[] {
  const previews = document.querySelectorAll(".theme-option > .theme-preview, .tray-option > .tray-swatches");
  return Array.from(previews, (preview) => {
    const card = preview.parentElement!;
    const style = getComputedStyle(card);
    const available = card.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
    return Math.round(available - preview.getBoundingClientRect().width);
  });
}
