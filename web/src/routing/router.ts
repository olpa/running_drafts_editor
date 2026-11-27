import {
  createRouter,
  createWebHistory,
  type RouteRecordRaw,
  type Router,
  type RouterHistory,
} from "vue-router";
import AccountPage from "../session/AccountPage.vue";
import FirstRunPage from "../session/FirstRunPage.vue";
import NotFoundPage from "../app/NotFoundPage.vue";
import PrivateAppShell from "../app/PrivateAppShell.vue";
import ProjectDetailPage from "../projects/ProjectDetailPage.vue";
import ProjectListPage from "../projects/ProjectListPage.vue";

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    name: "landing",
    component: FirstRunPage,
  },
  {
    path: "/projects",
    component: PrivateAppShell,
    meta: { private: true },
    sensitive: true,
    children: [
      { path: "", name: "projects", component: ProjectListPage },
      {
        path: ":projectId",
        name: "project-detail",
        component: ProjectDetailPage,
        props: true,
      },
    ],
  },
  {
    path: "/account",
    component: PrivateAppShell,
    meta: { private: true },
    children: [{ path: "", name: "account", component: AccountPage }],
  },
  {
    path: "/:pathMatch(.*)*",
    name: "not-found",
    component: NotFoundPage,
  },
];

export function createEditorRouter(history: RouterHistory = createWebHistory()): Router {
  return createRouter({
    history,
    routes,
    sensitive: true,
    scrollBehavior(_to, _from, savedPosition) {
      return savedPosition ?? { left: 0, top: 0 };
    },
  });
}
