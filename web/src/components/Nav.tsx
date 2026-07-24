import { Anchor, Button, Group, Text } from '@mantine/core';
import { Link, useNavigate } from 'react-router-dom';
import * as authApi from '../api/auth';
import { useAuth } from '../auth/AuthContext';

export function Nav() {
  const { account, setAccount } = useAuth();
  const navigate = useNavigate();

  async function handleLogout() {
    await authApi.logout();
    setAccount(null);
    navigate('/login');
  }

  return (
    <Group
      justify="space-between"
      py="md"
      px="lg"
      style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}
    >
      <Group>
        <Anchor component={Link} to="/" fw={700} underline="never">
          Visuara
        </Anchor>
        {account && (
          <Anchor component={Link} to="/">
            Dashboard
          </Anchor>
        )}
        <Anchor component={Link} to="/download">
          Download
        </Anchor>
        {account?.role === 'admin' && (
          <Anchor component={Link} to="/admin/settings">
            Admin
          </Anchor>
        )}
      </Group>
      <Group>
        {account ? (
          <>
            <Text size="sm" c="dimmed">
              {account.email}
            </Text>
            <Button variant="subtle" size="compact-sm" onClick={handleLogout}>
              Log out
            </Button>
          </>
        ) : (
          <Anchor component={Link} to="/login">
            Log in
          </Anchor>
        )}
      </Group>
    </Group>
  );
}
