import { Route, Routes } from 'react-router-dom';
import { AuthProvider } from './auth/AuthContext';
import { RequireAdmin, RequireAuth } from './auth/RouteGuards';
import { Nav } from './components/Nav';
import { AccountDetailPage } from './pages/admin/AccountDetailPage';
import { AccountsPage } from './pages/admin/AccountsPage';
import { AdminLayout } from './pages/admin/AdminLayout';
import { ClientBuildsPage } from './pages/admin/ClientBuildsPage';
import { RegistrationPage } from './pages/admin/RegistrationPage';
import { SettingsPage } from './pages/admin/SettingsPage';
import { DashboardPage } from './pages/DashboardPage';
import { DownloadPage } from './pages/DownloadPage';
import { LoginPage } from './pages/LoginPage';
import { RegisterPage } from './pages/RegisterPage';

export function App() {
  return (
    <AuthProvider>
      <Nav />
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/register" element={<RegisterPage />} />
        <Route path="/download" element={<DownloadPage />} />

        <Route element={<RequireAuth />}>
          <Route path="/" element={<DashboardPage />} />
        </Route>

        <Route element={<RequireAdmin />}>
          <Route path="/admin" element={<AdminLayout />}>
            <Route path="settings" element={<SettingsPage />} />
            <Route path="client-builds" element={<ClientBuildsPage />} />
            <Route path="accounts" element={<AccountsPage />} />
            <Route path="accounts/:id" element={<AccountDetailPage />} />
            <Route path="registration" element={<RegistrationPage />} />
          </Route>
        </Route>
      </Routes>
    </AuthProvider>
  );
}
