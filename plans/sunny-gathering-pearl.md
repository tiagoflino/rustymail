---
subject: Improve UpdateModal design to follow Apple HIG principles
description: Redesign the UpdateModal component in src/lib/components/UpdateModal.svelte to align more closely with macOS/Apple Human Interface Guidelines (HIG) for a more native-feeling experience. This includes improving typography, spacing, shadows, and button styles.
status: pending
---

# Context
The current `UpdateModal.svelte` uses a custom alert style that feels somewhat disconnected from the macOS aesthetic. The goal is to make it feel like a native system dialog or a highly polished macOS app component.

# Implementation Plan

## 1. Analysis & Research
- Review existing macOS/Apple HIG patterns for alerts and modal dialogs (e.g., spacing, corner radius, typography, button prominence).
- Analyze the current CSS in `UpdateModal.svelte` to identify areas of divergence from HIG.

## 2. Design Improvements
- **Typography**: Ensure usage of system fonts (`SF Pro`) with appropriate weights and sizes for titles vs. body text.
- **Layout & Spacing**: Adjust padding and margins to match the more breathable macOS style.
/
- **Visual Elements**:
    - Refine corner radius (macOS uses slightly larger, smoother radii).
    - Improve shadow depth (use layered shadows for a more natural look).
    - Update the icon container and SVG styling to be cleaner.
- **Buttons & Actions**:
    - Redesign buttons to follow macOS patterns (e.g., primary action prominence, secondary action subtlety).
    - Standardize button padding and font weight.
- **Release Notes Section**: Improve the "Release Notes" accordion/toggle style to feel more integrated.

## 3. Implementation Details
- Modify `src/lib/components/UpdateModal.svelte`.
- Update CSS classes: `.alert-container`, `.alert-content`, `.alert-actions`, `.btn-default`, etc.

## 4. Verification
- Verify the visual changes in the component.
- Ensure all existing functionality (onClose, onInstall, goToReleases) remains intact and works as expected.
