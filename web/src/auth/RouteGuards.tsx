import { Center, Loader } from '@mantine/core';
import { Navigate, Outlet, useLocation } from 'react-router-dom';
import { useAuth } from './AuthContext';

export function RequireAuth() {
  const { account, loading } = useAuth();
  const location = useLocation();

  if (loading) {
    return (
      <Center h="60vh">
        <Loader />
      </Center>
    );
  }
  if (!account) {
    return <Navigate to="/login" state={{ from: location }} replace />;
  }
  return <Outlet />;
}

export function RequireAdmin() {
  const { account, loading } = useAuth();
  const location = useLocation();

  if (loading) {
    return (
      <Center h="60vh">
        <Loader />
      </Center>
    );
  }
  if (!account) {
    return <Navigate to="/login" state={{ from: location }} replace />;
  }
  if (account.role !== 'admin') {
    return <Navigate to="/" replace />;
  }
  return <Outlet />;
}
