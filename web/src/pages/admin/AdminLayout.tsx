import { Container, Tabs } from '@mantine/core';
import { IconServer, IconSettings, IconUserPlus, IconUsers } from '@tabler/icons-react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';

const TABS = [
  { value: '/admin/settings', label: 'Settings', icon: IconSettings },
  { value: '/admin/client-builds', label: 'Client Builds', icon: IconServer },
  { value: '/admin/accounts', label: 'Accounts', icon: IconUsers },
  { value: '/admin/registration', label: 'Registration', icon: IconUserPlus },
];

export function AdminLayout() {
  const location = useLocation();
  const navigate = useNavigate();

  const active = TABS.find((t) => location.pathname.startsWith(t.value))?.value ?? TABS[0].value;

  return (
    <Container size="md" my={40}>
      <Tabs value={active} onChange={(value) => value && navigate(value)} mb="lg">
        <Tabs.List>
          {TABS.map((t) => (
            <Tabs.Tab key={t.value} value={t.value} leftSection={<t.icon size={16} />}>
              {t.label}
            </Tabs.Tab>
          ))}
        </Tabs.List>
      </Tabs>
      <Outlet />
    </Container>
  );
}
