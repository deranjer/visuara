import { Anchor, Badge, Button, Group, Modal, PasswordInput, Select, Stack, Table, TextInput } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { notifications } from '@mantine/notifications';
import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import * as adminApi from '../../api/admin';
import { ApiError } from '../../api/client';
import type { AccountRole, AccountSummaryView } from '../../api/types';

export function AccountsPage() {
  const [accounts, setAccounts] = useState<AccountSummaryView[]>([]);
  const [opened, { open, close }] = useDisclosure(false);
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [role, setRole] = useState<AccountRole>('user');
  const [creating, setCreating] = useState(false);

  async function load() {
    setAccounts(await adminApi.listAccounts());
  }

  useEffect(() => {
    load();
  }, []);

  async function handleCreate() {
    setCreating(true);
    try {
      await adminApi.createAccount(email, password, role);
      notifications.show({ message: 'Account created', color: 'green' });
      setEmail('');
      setPassword('');
      setRole('user');
      close();
      load();
    } catch (err) {
      notifications.show({
        message: err instanceof ApiError ? err.message : 'Failed to create account',
        color: 'red',
      });
    } finally {
      setCreating(false);
    }
  }

  return (
    <Stack>
      <Group justify="flex-end">
        <Button onClick={open}>New account</Button>
      </Group>
      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Email</Table.Th>
            <Table.Th>Role</Table.Th>
            <Table.Th>Created</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {accounts.map((a) => (
            <Table.Tr key={a.id}>
              <Table.Td>
                <Anchor component={Link} to={`/admin/accounts/${a.id}`}>
                  {a.email}
                </Anchor>
              </Table.Td>
              <Table.Td>
                <Badge color={a.role === 'admin' ? 'blue' : 'gray'}>{a.role}</Badge>
              </Table.Td>
              <Table.Td>{new Date(a.created_at * 1000).toLocaleString()}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>

      <Modal opened={opened} onClose={close} title="New account">
        <Stack>
          <TextInput label="Email" value={email} onChange={(e) => setEmail(e.currentTarget.value)} required />
          <PasswordInput
            label="Password"
            value={password}
            onChange={(e) => setPassword(e.currentTarget.value)}
            required
          />
          <Select
            label="Role"
            data={[
              { value: 'user', label: 'User' },
              { value: 'admin', label: 'Admin' },
            ]}
            value={role}
            onChange={(v) => setRole((v as AccountRole) ?? 'user')}
            allowDeselect={false}
          />
          <Button onClick={handleCreate} loading={creating}>
            Create
          </Button>
        </Stack>
      </Modal>
    </Stack>
  );
}
