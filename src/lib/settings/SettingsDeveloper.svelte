<script>
  let {
    devNativeContextMenu = false,
    onDevNativeContextMenuChange = () => {},
  } = $props();
</script>

<h3>Developer</h3>
<p class="settings-notice">These settings are only visible in dev builds.</p>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Use native context menu</span>
    <span class="setting-desc">
      Disables the custom context menu and restores the native WebView2 menu,
      which includes Inspect Element. Useful for debugging layout and styles.
    </span>
  </div>
  <label class="toggle">
    <input
      type="checkbox"
      checked={devNativeContextMenu}
      onchange={(e) => onDevNativeContextMenuChange(e.currentTarget.checked)}
    />
    <span class="toggle-label">{devNativeContextMenu ? 'On' : 'Off'}</span>
  </label>
</div>
