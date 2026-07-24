import { Anchor, Container, Group } from '@mantine/core';
import { Link, Outlet, useLocation } from 'react-router-dom';

const TABS = [
  { value: '/admin/settings', label: 'Settings' },
  { value: '/admin/client-builds', label: 'Client Builds' },
  { value: '/admin/accounts', label: 'Accounts' },
  { value: '/admin/registration', label: 'Registration' },
];

export function AdminLayout() {
  const location = useLocation();

  return (
    <Container size="md" my={40}>
      <Group mb="lg" gap="lg" style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }} pb="sm">
        {TABS.map((t) => {
          const active = location.pathname.startsWith(t.value);
          return (
            <Anchor
              key={t.value}
              component={Link}
              to={t.value}
              fw={active ? 700 : 400}
              c={active ? undefined : 'dimmed'}
              underline="never"
            >
              {t.label}
            </Anchor>
          );
        })}
      </Group>
      <Outlet />
    </Container>
  );
}
