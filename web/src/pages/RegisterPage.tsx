import { Alert, Anchor, Button, Center, Container, Loader, Paper, PasswordInput, Text, TextInput, Title } from '@mantine/core';
import { type FormEvent, useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import * as authApi from '../api/auth';
import { ApiError } from '../api/client';
import { useAuth } from '../auth/AuthContext';

export function RegisterPage() {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [statusLoading, setStatusLoading] = useState(true);
  const [enabled, setEnabled] = useState(true);
  const { setAccount } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    authApi
      .registrationStatus()
      .then((s) => setEnabled(s.enabled))
      .catch(() => setEnabled(true))
      .finally(() => setStatusLoading(false));
  }, []);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      const account = await authApi.register(email, password);
      setAccount(account);
      navigate('/', { replace: true });
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Failed to register');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Container size={420} my={40}>
      <Title ta="center">Create an account</Title>
      <Paper withBorder shadow="sm" p={30} mt={30} radius="md">
        {statusLoading ? (
          <Center py="xl">
            <Loader />
          </Center>
        ) : !enabled ? (
          <Alert color="yellow" title="Registration closed">
            Public sign-up is currently disabled — contact your administrator for an invite.
          </Alert>
        ) : (
          <form onSubmit={handleSubmit}>
            {error && (
              <Alert color="red" mb="md">
                {error}
              </Alert>
            )}
            <TextInput
              label="Email"
              type="email"
              value={email}
              onChange={(e) => setEmail(e.currentTarget.value)}
              autoFocus
              required
            />
            <PasswordInput
              label="Password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
              required
              mt="md"
            />
            <Button type="submit" fullWidth mt="xl" loading={submitting}>
              Sign up
            </Button>
          </form>
        )}
        <Text ta="center" mt="md" size="sm">
          Already have an account?{' '}
          <Anchor component={Link} to="/login">
            Log in
          </Anchor>
        </Text>
      </Paper>
    </Container>
  );
}
