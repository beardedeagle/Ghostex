// Sample Markdown for `?fixtures=1`: a Linear description, a comment and a PR body that use every block the Work page draws.

export const MARKDOWN_DESCRIPTION = `People want to copy a share link **without** opening the share dialog. See the [design notes](https://www.figma.com/file/copy-link) and https://linear.app/shortpoint/issue/spx-1201.

## Scope

* Add *Copy link* to the share menu
* Show a short \`Link copied\` toast
  * Hide it after 2 seconds
* ~~Change the share dialog~~ (out of scope)

### Steps

1. Open a page in EasyPass
2. Open the share menu
3. Pick **Copy link**

### Checklist

- [x] Design approved
- [ ] Copy text reviewed
- [ ] QC video recorded

| Browser | Clipboard API | Notes |
| --- | --- | --- |
| Chrome | Yes | |
| Edge | Yes | Same as Chrome |
| Safari | Partial | Needs a user gesture |

> The toast follows the existing EasyPass toast style.

\`\`\`ts
await navigator.clipboard.writeText(shareUrl);
showToast("Link copied");
\`\`\`

---

Prices in the copy stay as typed: $5 and $10.`;

export const MARKDOWN_COMMENT =
  'Looks good. Two notes:\n\n1. Keep the **existing** menu order\n2. Use `aria-live` for the toast\n\nRef: https://developer.mozilla.org/docs/Web/API/Clipboard';

export const MARKDOWN_PR_BODY = `## Summary

Fixes **SPX-1234**: the Table element no longer remounts the page canvas on first layout.

## Changes

- Keep the canvas mounted while the table measures itself
- Add an e2e test for Live mode with a table

## Test plan

- [x] Unit tests
- [x] \`e2e-live-mode\`
- [ ] Manual check in Safari`;
