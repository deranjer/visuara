import { ActionIcon, Container, Group, Table, Text, Title } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useEffect, useState } from 'react';
import * as devicesApi from '../api/devices';
import type { DeviceSummary } from '../api/types';
import { useAuth } from '../auth/AuthContext';
import { StatusDot } from '../components/StatusDot';

export function DashboardPage() {
  const { account } = useAuth();
  const [devices, setDevices] = useState<DeviceSummary[] | null>(null);

  async function load() {
    try {
      setDevices(await devicesApi.listDevices());
    } catch {
      setDevices([]);
    }
  }

  useEffect(() => {
    load();
  }, []);

  async function handleDelete(deviceId: string) {
    if (!confirm('Delete this device?')) return;
    try {
      await devicesApi.deleteDevice(deviceId);
      notifications.show({ message: 'Device removed', color: 'green' });
      load();
    } catch {
      notifications.show({ message: 'Failed to remove device', color: 'red' });
    }
  }

  return (
    <Container size="md" my={40}>
      <Title mb="md">Welcome, {account?.email}</Title>
      {devices === null ? (
        <Text>Loading…</Text>
      ) : devices.length === 0 ? (
        <Text c="dimmed">
          No devices yet — share a machine from the desktop client&apos;s Host tab, then it&apos;ll show up here.
        </Text>
      ) : (
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Device</Table.Th>
              <Table.Th>Status</Table.Th>
              <Table.Th />
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {devices.map((d) => (
              <Table.Tr key={d.device_id}>
                <Table.Td>{d.name}</Table.Td>
                <Table.Td>
                  <StatusDot online={d.online} />
                </Table.Td>
                <Table.Td>
                  <Group justify="flex-end">
                    <ActionIcon color="red" variant="subtle" onClick={() => handleDelete(d.device_id)} aria-label="Delete device">
                      ×
                    </ActionIcon>
                  </Group>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
    </Container>
  );
}
