<script>
  let logFileAccess = $state(true);

  $effect(() => {
    invoke('get_setting', { key: 'log_file_access' })
      .then(v => { if (v !== '') logFileAccess = v === 'true'; })
      .catch(() => {});
  });

  $effect(() => {
    invoke('set_setting', { key: 'log_file_access', value: String(logFileAccess) }).catch(() => {});
  });
</script>

<h3>Privacy</h3>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Log file access</span>
    <span class="setting-desc">
      Records every file the app reads to a local audit log stored on this machine.
    </span>
  </div>
  <label class="toggle">
    <input type="checkbox" bind:checked={logFileAccess} />
    <span class="toggle-label">{logFileAccess ? 'On' : 'Off'}</span>
  </label>
</div>

<div class="setting-row">
  <div class="setting-label">
    <span class="setting-name">Local only</span>
    <span class="setting-desc">No data ever leaves this machine. Cannot be disabled.</span>
  </div>
  <label class="toggle toggle-locked">
    <input type="checkbox" checked disabled />
    <span class="toggle-label">Always on</span>
  </label>
</div>
