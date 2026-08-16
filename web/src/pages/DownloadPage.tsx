import { Badge, Button, Card, Code, Container, Group, Stack, Text, Title } from '@mantine/core';
import { useEffect, useState } from 'react';
import * as downloadApi from '../api/download';
import type { DownloadListView } from '../api/types';

export function DownloadPage() {
  const [data, setData] = useState<DownloadListView | null>(null);

  useEffect(() => {
    downloadApi
      .listDownloads()
      .then(setData)
      .catch(() => setData({ server_url: null, options: [] }));
  }, []);

  return (
    <Container size="sm" my={40}>
      <Title mb="xs">Download Visuara</Title>
      <Text c="dimmed" mb="lg">
        Downloads below connect to: <Code>{data?.server_url || '(not configured yet)'}</Code>
      </Text>
      {data === null ? (
        <Text>Loading…</Text>
      ) : (
        <Stack>
          {data.options.map((o) => (
            <Card key={o.platform} withBorder padding="lg">
              <Group justify="space-between">
                <Stack gap={2}>
                  <Group gap="xs">
                    <Text fw={600}>{o.platform}</Text>
                    {o.recommended && <Badge color="blue">Recommended for your system</Badge>}
                  </Group>
                  {o.available && (
                    <Text size="sm" c="dimmed">
                      {o.custom_build ? 'custom build' : o.version ?? ''}
                    </Text>
                  )}
                </Stack>
                {o.available ? (
                  <Button component="a" href={downloadApi.downloadUrl(o.platform)}>
                    Download
                  </Button>
                ) : (
                  <Text c="dimmed" size="sm">
                    not available yet
                  </Text>
                )}
              </Group>
            </Card>
          ))}
        </Stack>
      )}
    </Container>
  );
}
