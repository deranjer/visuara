import { Anchor, Button, Group, Select, Stack, Table, Text, Title } from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useEffect, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import * as adminApi from '../../api/admin';
import { ApiError } from '../../api/client';
import type { AccountDetailView, AccountRole } from '../../api/types';
import { StatusDot } from '../../components/StatusDot';

export function AccountDetailPage() {
  const { id } = useParams<{ id: string }>();
  const accountId = Number(id);
  const [detail, setDetail] = useState<AccountDetailView | null>(null);
  const [savingRole, setSavingRole] = useState(false);
  const navigate = useNavigate();

  async function load() {
    setDetail(await adminApi.accountDetail(accountId));
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accountId]);

  async function handleDeleteDevice(deviceId: string) {
    if (!confirm('Delete this device?')) return;
    await adminApi.deleteAccountDevice(accountId, deviceId);
    load();
  }

  async function handleRoleChange(value: string | null) {
    if (!value || !detail) return;
    const role = value as AccountRole;
    setSavingRole(true);
    try {
      await adminApi.setAccountRole(accountId, role);
      setDetail({ ...detail, role });
      notifications.show({ message: 'Role updated', color: 'green' });
    } catch (err) {
      notifications.show({
        message: err instanceof ApiError ? err.message : 'Failed to update role',
        color: 'red',
      });
    } finally {
      setSavingRole(false);
    }
  }

  async function handleDeleteAccount() {
    if (!confirm('Delete this account and all its devices? This cannot be undone.')) return;
    try {
      await adminApi.deleteAccount(accountId);
      notifications.show({ message: 'Account deleted', color: 'green' });
      navigate('/admin/accounts');
    } catch (err) {
      notifications.show({
        message: err instanceof ApiError ? err.message : 'Failed to delete account',
        color: 'red',
      });
    }
  }

  if (!detail) return null;

  return (
    <Stack>
      <Title order={3}>{detail.email}</Title>
      <Group justify="space-between" align="flex-end">
        <Select
          label="Role"
          data={[
            { value: 'user', label: 'User' },
            { value: 'admin', label: 'Admin' },
          ]}
          value={detail.role}
          onChange={handleRoleChange}
          disabled={savingRole}
          allowDeselect={false}
          w={200}
        />
        <Button color="red" variant="outline" onClick={handleDeleteAccount}>
          Delete account
        </Button>
      </Group>
      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Name</Table.Th>
            <Table.Th>Device ID</Table.Th>
            <Table.Th>Status</Table.Th>
            <Table.Th>Unattended</Table.Th>
            <Table.Th />
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {detail.devices.length === 0 ? (
            <Table.Tr>
              <Table.Td colSpan={5}>
                <Text c="dimmed">No devices.</Text>
              </Table.Td>
            </Table.Tr>
          ) : (
            detail.devices.map((d) => (
              <Table.Tr key={d.id}>
                <Table.Td>{d.name}</Table.Td>
                <Table.Td>{d.id}</Table.Td>
                <Table.Td>
                  <StatusDot online={d.online} />
                </Table.Td>
                <Table.Td>{d.unattended_access_enabled ? 'yes' : 'no'}</Table.Td>
                <Table.Td>
                  <Button size="xs" color="red" variant="subtle" onClick={() => handleDeleteDevice(d.id)}>
                    Delete
                  </Button>
                </Table.Td>
              </Table.Tr>
            ))
          )}
        </Table.Tbody>
      </Table>
      <Anchor component={Link} to="/admin/accounts">
        Back to accounts
      </Anchor>
    </Stack>
  );
}
