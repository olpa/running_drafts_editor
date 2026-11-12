import {
  createRouter,
  createWebHistory,
  type RouteRecordRaw,
  type Router,
  type RouterHistory,
} from "vue-router";
import NotFoundPage from "../app/NotFoundPage.vue";
import PrivateAppShell from "../app/PrivateAppShell.vue";
import ProjectDetailPage from "../projects/ProjectDetailPage.vue";
import ProjectListPage from "../projects/ProjectListPage.vue";

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    redirect: (to) => ({ name: "projects", query: to.query, hash: to.hash }),
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
