import { AppShell, Burger, Button, Group, NavLink, Stack, Text } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import {
  IconDownload,
  IconLayoutDashboard,
  IconLogin,
  IconShieldLock,
} from '@tabler/icons-react';
import type { ReactNode } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import * as authApi from '../api/auth';
import { useAuth } from '../auth/AuthContext';

export function AppLayout({ children }: { children: ReactNode }) {
  const { account, setAccount } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [mobileOpened, { toggle: toggleMobile }] = useDisclosure(false);
  const [desktopOpened, { toggle: toggleDesktop }] = useDisclosure(true);

  async function handleLogout() {
    await authApi.logout();
    setAccount(null);
    navigate('/login');
  }

  return (
    <AppShell
      header={{ height: 60 }}
      navbar={{
        width: 240,
        breakpoint: 'sm',
        collapsed: { mobile: !mobileOpened, desktop: !desktopOpened },
      }}
      padding="md"
    >
      <AppShell.Header>
        <Group h="100%" px="md" justify="space-between">
          <Group>
            <Burger opened={mobileOpened} onClick={toggleMobile} hiddenFrom="sm" size="sm" />
            <Burger opened={desktopOpened} onClick={toggleDesktop} visibleFrom="sm" size="sm" />
            <Text component={Link} to="/" fw={700} td="none" c="inherit">
              Visuara
            </Text>
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
              <NavLink
                component={Link}
                to="/login"
                label="Log in"
                leftSection={<IconLogin size={16} />}
                variant="subtle"
                style={{ borderRadius: 4 }}
              />
            )}
          </Group>
        </Group>
      </AppShell.Header>

      <AppShell.Navbar p="md">
        <Stack h="100%" justify="space-between">
          <Stack gap={4}>
            {account && (
              <NavLink
                component={Link}
                to="/"
                label="Dashboard"
                leftSection={<IconLayoutDashboard size={18} />}
                active={location.pathname === '/'}
              />
            )}
            <NavLink
              component={Link}
              to="/download"
              label="Download"
              leftSection={<IconDownload size={18} />}
              active={location.pathname === '/download'}
            />
          </Stack>

          {account?.role === 'admin' && (
            <NavLink
              component={Link}
              to="/admin/settings"
              label="Admin"
              leftSection={<IconShieldLock size={18} />}
              active={location.pathname.startsWith('/admin')}
            />
          )}
        </Stack>
      </AppShell.Navbar>

      <AppShell.Main>{children}</AppShell.Main>
    </AppShell>
  );
}
