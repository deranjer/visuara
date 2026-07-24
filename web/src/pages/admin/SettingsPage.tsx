import { Button, Stack, TextInput } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useEffect, useState } from 'react';
import * as adminApi from '../../api/admin';

export function SettingsPage() {
  const [serverUrl, setServerUrl] = useState('');
  const [deviceName, setDeviceName] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    adminApi
      .getSettings()
      .then((s) => {
        setServerUrl(s.server_url);
        setDeviceName(s.default_device_name);
      })
      .finally(() => setLoading(false));
  }, []);

  async function handleSave() {
    setSaving(true);
    try {
      await adminApi.saveSettings(serverUrl, deviceName);
      notifications.show({ message: 'Settings saved', color: 'green' });
    } catch {
      notifications.show({ message: 'Failed to save settings', color: 'red' });
    } finally {
      setSaving(false);
    }
  }

  if (loading) return null;

  return (
    <Stack maw={480}>
      <TextInput
        label="Server URL embedded in downloaded clients"
        placeholder="wss://visuara.example.com/ws"
        value={serverUrl}
        onChange={(e) => setServerUrl(e.currentTarget.value)}
      />
      <TextInput
        label="Default device name"
        placeholder="this-machine"
        value={deviceName}
        onChange={(e) => setDeviceName(e.currentTarget.value)}
      />
      <Button onClick={handleSave} loading={saving} w={120}>
        Save
      </Button>
    </Stack>
  );
}
