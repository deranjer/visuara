import { Button, Stack, Table, Text } from '@mantine/core';
import { useEffect, useState } from 'react';
import * as adminApi from '../../api/admin';
import type { ClientBuildStatus } from '../../api/types';

export function ClientBuildsPage() {
  const [statuses, setStatuses] = useState<ClientBuildStatus[]>([]);
  const [fetching, setFetching] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  async function load() {
    setStatuses(await adminApi.clientBuilds());
  }

  useEffect(() => {
    load();
  }, []);

  async function handleFetch() {
    setFetching(true);
    setNotice(null);
    try {
      const results = await adminApi.fetchRelease();
      setNotice(
        results.map((r) => (r.error ? `${r.platform}: ${r.error}` : `${r.platform}: fetched ${r.version}`)).join(' | '),
      );
      load();
    } catch {
      setNotice('failed to check GitHub releases');
    } finally {
      setFetching(false);
    }
  }

  return (
    <Stack>
      {notice && <Text size="sm">{notice}</Text>}
      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Platform</Table.Th>
            <Table.Th>Status</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {statuses.map((s) => (
            <Table.Tr key={s.platform}>
              <Table.Td>{s.platform}</Table.Td>
              <Table.Td>{s.status}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      <Button onClick={handleFetch} loading={fetching} w={260}>
        Check GitHub for latest release
      </Button>
      <Text size="sm" c="dimmed">
        A manually-placed file in the operator&apos;s client-templates directory always takes priority over a fetched
        one.
      </Text>
    </Stack>
  );
}
