import { Button, Group, Stack, Switch, Text } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useEffect, useState } from 'react';
import * as adminApi from '../../api/admin';

export function RegistrationPage() {
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    adminApi
      .getRegistration()
      .then((s) => setEnabled(s.enabled))
      .finally(() => setLoading(false));
  }, []);

  async function handleToggle() {
    const next = !enabled;
    setSaving(true);
    try {
      const result = await adminApi.setRegistration(next);
      setEnabled(result.enabled);
      notifications.show({
        message: result.enabled ? 'Public registration enabled' : 'Public registration disabled',
        color: 'green',
      });
    } catch {
      notifications.show({ message: 'Failed to update registration setting', color: 'red' });
    } finally {
      setSaving(false);
    }
  }

  if (loading) return null;

  return (
    <Stack maw={480}>
      <Group justify="space-between">
        <div>
          <Text fw={500}>Public self-service registration</Text>
          <Text size="sm" c="dimmed">
            When off, the sign-up page is closed and only admins can create new accounts.
          </Text>
        </div>
        <Switch checked={enabled} onChange={handleToggle} disabled={saving} size="md" />
      </Group>
      <Button variant="light" onClick={handleToggle} loading={saving} w={220}>
        {enabled ? 'Disable registration' : 'Re-enable registration'}
      </Button>
    </Stack>
  );
}
